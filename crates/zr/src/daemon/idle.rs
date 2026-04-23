use std::collections::HashMap;
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::module_cache::{stop_warm_library_instance, LibraryPool};
use super::service_supervisor::subprocess::{backoff_duration, check_health, wait_for_health};
use super::service_supervisor::{ManagedKind, ManagedService, ServiceState};
use super::{
    DaemonControl, DAEMON_PORT, DEFAULT_DRAIN_TIMEOUT_SECS, DEFAULT_IDLE_TIMEOUT_SECS,
    HEALTH_CHECK_INTERVAL, HEALTH_CHECK_TIMEOUT, MAX_RESTART_FAILURES,
};

fn idle_timeout() -> Option<Duration> {
    let secs = std::env::var("ZR_DAEMON_IDLE_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_IDLE_TIMEOUT_SECS);
    if secs == 0 {
        None
    } else {
        Some(Duration::from_secs(secs))
    }
}

pub(super) fn health_monitor_loop(
    services: Arc<Mutex<HashMap<String, ManagedService>>>,
    library_pools: Arc<Mutex<HashMap<String, LibraryPool>>>,
    control: Arc<DaemonControl>,
    last_activity: Arc<Mutex<Instant>>,
) {
    loop {
        std::thread::sleep(HEALTH_CHECK_INTERVAL);

        if control.should_shutdown_all() {
            break;
        }

        let names: Vec<String> = {
            let svcs = services.lock().unwrap();
            svcs.keys().cloned().collect()
        };
        reap_idle_library_instances(&library_pools);
        let library_pool_count = {
            let pools = library_pools.lock().unwrap();
            pools.values().map(|pool| pool.instances.len()).sum::<usize>()
        };

        if names.is_empty()
            && library_pool_count == 0
            && let Some(timeout) = idle_timeout()
        {
            let idle = last_activity.lock().unwrap().elapsed();
            if idle >= timeout {
                eprintln!("daemon: idle for {:?} with no services — exiting", idle);
                control.shutdown_all();
                let _ = TcpStream::connect(format!("127.0.0.1:{}", DAEMON_PORT));
                return;
            }
        }

        if control.is_dispatch_draining() {
            let drain_timeout_secs = std::env::var("ZR_DAEMON_DRAIN_TIMEOUT_SECS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(DEFAULT_DRAIN_TIMEOUT_SECS);
            let drain_timeout = Duration::from_secs(drain_timeout_secs);
            let no_services = names.is_empty() && library_pool_count == 0;
            let drain_expired = control
                .drain_started_at()
                .is_some_and(|started| started.elapsed() >= drain_timeout);

            if no_services || drain_expired {
                if drain_expired && !no_services {
                    eprintln!(
                        "daemon: drain timeout reached with active services; forcing shutdown"
                    );
                } else {
                    eprintln!("daemon: dispatch drain complete; exiting");
                }
                control.shutdown_all();
                let _ = TcpStream::connect(format!("127.0.0.1:{}", DAEMON_PORT));
                return;
            }
        }

        for name in names {
            if control.should_shutdown_all() {
                return;
            }

            let mut svcs = services.lock().unwrap();
            let Some(svc) = svcs.get_mut(&name) else {
                continue;
            };

            if svc.state == ServiceState::Failed {
                continue;
            }

            let sub = match &mut svc.kind {
                ManagedKind::Subprocess(s) => s,
                ManagedKind::Wasm(_) => continue,
            };

            let process_dead = sub
                .process
                .as_mut()
                .is_some_and(|c| c.try_wait().ok().flatten().is_some());

            let healthy = if process_dead {
                false
            } else {
                check_health(svc.port, &svc.health_path)
            };

            if healthy {
                svc.consecutive_failures = 0;
                svc.state = ServiceState::Running;
                continue;
            }

            svc.consecutive_failures += 1;
            eprintln!(
                "daemon: service '{}' unhealthy (failure {}/{})",
                name, svc.consecutive_failures, MAX_RESTART_FAILURES
            );

            if svc.consecutive_failures >= MAX_RESTART_FAILURES {
                eprintln!(
                    "daemon: service '{}' marked as failed after {} consecutive failures",
                    name, MAX_RESTART_FAILURES
                );
                svc.state = ServiceState::Failed;
                if let Some(ref mut child) = sub.process {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                continue;
            }

            if let Some(ref mut child) = sub.process {
                let _ = child.kill();
                let _ = child.wait();
            }

            let wait = backoff_duration(svc.consecutive_failures);
            let bin_path = sub.bin_path.clone();
            let port = svc.port;
            let health_path = svc.health_path.clone();
            let svc_name = name.clone();

            drop(svcs);

            eprintln!("daemon: restarting '{}' in {:?}", svc_name, wait);
            std::thread::sleep(wait);

            if control.should_shutdown_all() {
                return;
            }

            match Command::new(&bin_path)
                .arg(format!("--listen=:{}", port))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
            {
                Ok(child) => {
                    if wait_for_health(port, &health_path, HEALTH_CHECK_TIMEOUT) {
                        let mut svcs = services.lock().unwrap();
                        if let Some(svc) = svcs.get_mut(&svc_name)
                            && let ManagedKind::Subprocess(sub) = &mut svc.kind
                        {
                            sub.process = Some(child);
                            svc.state = ServiceState::Running;
                            svc.consecutive_failures = 0;
                            eprintln!("daemon: service '{}' restarted successfully", svc_name);
                        }
                    } else {
                        eprintln!(
                            "daemon: service '{}' failed health check after restart",
                            svc_name
                        );
                    }
                }
                Err(e) => {
                    eprintln!("daemon: failed to restart '{}': {}", svc_name, e);
                }
            }
        }
    }
}

fn reap_idle_library_instances(library_pools: &Arc<Mutex<HashMap<String, LibraryPool>>>) {
    let mut to_stop = Vec::new();
    {
        let mut pools = library_pools.lock().unwrap();
        let keys = pools.keys().cloned().collect::<Vec<_>>();
        for key in keys {
            let Some(pool) = pools.get_mut(&key) else {
                continue;
            };

            let idle_timeout = pool.idle_timeout;
            let mut retained = Vec::new();
            for instance in pool.instances.drain(..) {
                let idle = instance.last_used.lock().unwrap().elapsed();
                if !instance.busy.load(std::sync::atomic::Ordering::Acquire) && idle >= idle_timeout {
                    to_stop.push(instance);
                } else {
                    retained.push(instance);
                }
            }
            pool.instances = retained;
        }
        pools.retain(|_, pool| !pool.instances.is_empty());
    }

    for instance in to_stop {
        stop_warm_library_instance(instance);
    }
}
