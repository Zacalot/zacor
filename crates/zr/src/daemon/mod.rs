use crate::error::*;
use std::collections::HashMap;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zacor_host::protocol::DaemonRefusal;

mod dispatch;
mod capability_router;
mod idle;
mod module_cache;
mod protocol;
mod server;
mod service_supervisor;

use module_cache::{stop_warm_library_instance, LibraryPool};
use protocol::{DaemonRequest, DaemonResponse, ServiceStatusEntry};
use service_supervisor::{stop_managed, ManagedService};

const DAEMON_PORT: u16 = 19100;
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(10);
const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESTART_FAILURES: u32 = 5;
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 30 * 60;
const DEFAULT_DRAIN_TIMEOUT_SECS: u64 = 60;

struct DaemonControl {
    shutdown_all: AtomicBool,
    shutdown_dispatch: AtomicBool,
    drain_refusal: Mutex<Option<DaemonRefusal>>,
    drain_started_at: Mutex<Option<Instant>>,
}

impl DaemonControl {
    fn new() -> Self {
        Self {
            shutdown_all: AtomicBool::new(false),
            shutdown_dispatch: AtomicBool::new(false),
            drain_refusal: Mutex::new(None),
            drain_started_at: Mutex::new(None),
        }
    }

    fn shutdown_all(&self) {
        self.shutdown_all.store(true, Ordering::Release);
    }

    fn should_shutdown_all(&self) -> bool {
        self.shutdown_all.load(Ordering::Acquire)
    }

    fn begin_dispatch_drain(&self, refusal: DaemonRefusal) {
        self.shutdown_dispatch.store(true, Ordering::Release);
        *self.drain_refusal.lock().unwrap() = Some(refusal);
        let mut started_at = self.drain_started_at.lock().unwrap();
        if started_at.is_none() {
            *started_at = Some(Instant::now());
        }
    }

    fn is_dispatch_draining(&self) -> bool {
        self.shutdown_dispatch.load(Ordering::Acquire)
    }

    fn drain_started_at(&self) -> Option<Instant> {
        *self.drain_started_at.lock().unwrap()
    }

    fn dispatch_refusal(&self, client_version: Option<&str>) -> Option<DaemonRefusal> {
        if !self.is_dispatch_draining() {
            return None;
        }

        match client_version {
            Some(client) if client != env!("CARGO_PKG_VERSION") => {
                Some(DaemonRefusal::VersionMismatch {
                    daemon: env!("CARGO_PKG_VERSION").into(),
                    client: client.to_string(),
                })
            }
            _ => self.drain_refusal.lock().unwrap().clone().or(Some(DaemonRefusal::Other {
                message: "daemon draining after version mismatch; retry shortly".into(),
            })),
        }
    }
}

pub struct DaemonServer {
    home: PathBuf,
    services: Arc<Mutex<HashMap<String, ManagedService>>>,
    library_pools: Arc<Mutex<HashMap<String, LibraryPool>>>,
    capabilities: Arc<capability_router::CapabilityRouter>,
    control: Arc<DaemonControl>,
    last_activity: Arc<Mutex<Instant>>,
}

impl DaemonServer {
    pub fn new(home: PathBuf) -> Self {
        DaemonServer {
            home,
            services: Arc::new(Mutex::new(HashMap::new())),
            library_pools: Arc::new(Mutex::new(HashMap::new())),
            capabilities: Arc::new(capability_router::CapabilityRouter::new()),
            control: Arc::new(DaemonControl::new()),
            last_activity: Arc::new(Mutex::new(Instant::now())),
        }
    }

    pub fn run(&self) -> Result<()> {
        let addr = format!("127.0.0.1:{}", DAEMON_PORT);
        let listener = TcpListener::bind(&addr).with_context(|| {
            format!(
                "failed to bind daemon on {} — is another daemon running?",
                addr
            )
        })?;

        eprintln!("daemon: listening on {}", addr);
        listener.set_nonblocking(false).ok();

        let services = self.services.clone();
        let library_pools = self.library_pools.clone();
        let control = self.control.clone();
        let last_activity = self.last_activity.clone();
        std::thread::Builder::new()
            .name("zr-health-monitor".into())
            .spawn(move || idle::health_monitor_loop(services, library_pools, control, last_activity))
            .context("failed to spawn health monitor")?;

        for stream in listener.incoming() {
            if self.control.should_shutdown_all() {
                break;
            }
            match stream {
                Ok(stream) => {
                    let services = self.services.clone();
                    let library_pools = self.library_pools.clone();
                    let capabilities = self.capabilities.clone();
                    let control = self.control.clone();
                    let last_activity = self.last_activity.clone();
                    let home = self.home.clone();
                    std::thread::Builder::new()
                        .name("zr-daemon-conn".into())
                        .spawn(move || {
                            if let Err(e) = server::handle_connection(
                                stream,
                                &services,
                                &library_pools,
                                &capabilities,
                                &control,
                                &last_activity,
                                &home,
                            ) {
                                eprintln!("daemon: connection error: {:#}", e);
                            }
                        })
                        .ok();
                }
                Err(e) => {
                    if self.control.should_shutdown_all() {
                        break;
                    }
                    eprintln!("daemon: accept error: {}", e);
                }
            }
        }

        self.stop_all_services();
        eprintln!("daemon: stopped");
        Ok(())
    }

    fn stop_all_services(&self) {
        let mut services = self.services.lock().unwrap();
        let drained: Vec<(String, ManagedService)> = services.drain().collect();
        drop(services);
        for (name, svc) in drained {
            eprintln!("daemon: stopping service '{}'", name);
            stop_managed(svc);
        }

        let mut pools = self.library_pools.lock().unwrap();
        let drained_pools: Vec<LibraryPool> = pools.drain().map(|(_, pool)| pool).collect();
        drop(pools);
        for pool in drained_pools {
            for instance in pool.instances {
                stop_warm_library_instance(instance);
            }
        }
    }
}

#[cfg(test)]
mod tests;
