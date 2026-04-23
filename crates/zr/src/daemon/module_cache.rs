use crate::error::*;
use crate::package_definition::{PackageDefinition, ServiceSection};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zacor_package::protocol::Message;

use super::service_supervisor::wasm::is_done_frame;

pub(super) struct WarmLibraryInstance {
    pub(super) writer: Mutex<crate::wasm_runtime::BridgedWriter>,
    pub(super) reader: Mutex<BufReader<crate::wasm_runtime::BridgedReader>>,
    controller: Mutex<Option<crate::wasm_runtime::Controller>>,
    pub(super) busy: AtomicBool,
    pub(super) last_used: Mutex<Instant>,
}

pub(super) struct LibraryPool {
    pub(super) idle_timeout: Duration,
    pub(super) max_concurrent: usize,
    pub(super) instances: Vec<Arc<WarmLibraryInstance>>,
}

pub(super) fn acquire_warm_library_instance(
    library_pools: &Arc<Mutex<HashMap<String, LibraryPool>>>,
    home: &Path,
    package: &str,
    version: &str,
    def: &PackageDefinition,
    service: &ServiceSection,
    env: &HashMap<String, String>,
) -> Result<Arc<WarmLibraryInstance>> {
    let key = warm_library_pool_key(package, version, env);
    let idle_timeout = Duration::from_secs(service.idle_timeout_secs.unwrap_or(600));
    let max_concurrent = service.max_concurrent.unwrap_or(4).max(1);

    loop {
        let mut should_spawn = false;
        {
            let mut pools = library_pools.lock().unwrap();
            let pool = pools.entry(key.clone()).or_insert_with(|| LibraryPool {
                idle_timeout,
                max_concurrent,
                instances: Vec::new(),
            });

            for instance in &pool.instances {
                if instance
                    .busy
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    *instance.last_used.lock().unwrap() = Instant::now();
                    return Ok(instance.clone());
                }
            }

            if pool.instances.len() < pool.max_concurrent {
                should_spawn = true;
            }
        }

        if should_spawn {
            let instance = spawn_warm_library_instance(home, package, version, def, env)?;
            instance.busy.store(true, Ordering::Release);
            let mut pools = library_pools.lock().unwrap();
            let pool = pools.entry(key.clone()).or_insert_with(|| LibraryPool {
                idle_timeout,
                max_concurrent,
                instances: Vec::new(),
            });
            pool.instances.push(instance.clone());
            return Ok(instance);
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}

fn spawn_warm_library_instance(
    home: &Path,
    package: &str,
    version: &str,
    def: &PackageDefinition,
    env: &HashMap<String, String>,
) -> Result<Arc<WarmLibraryInstance>> {
    let wasm_name = def
        .wasm
        .as_ref()
        .ok_or_else(|| anyhow!("library service '{}' has no wasm", package))?;
    let wasm_path = crate::paths::store_wasm_path(home, package, version, wasm_name);
    if !wasm_path.exists() {
        bail!("wasm artifact missing: {}", wasm_path.display());
    }

    let host = crate::wasm_runtime::WasmHost::shared()?;
    let module = host.load_module(&wasm_path)?;
    let env: Vec<(String, String)> = env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let session = host.invoke(module, env)?;
    let crate::wasm_runtime::WasmSession {
        writer,
        reader,
        controller,
    } = session;

    Ok(Arc::new(WarmLibraryInstance {
        writer: Mutex::new(writer),
        reader: Mutex::new(reader),
        controller: Mutex::new(Some(controller)),
        busy: AtomicBool::new(false),
        last_used: Mutex::new(Instant::now()),
    }))
}

pub(super) fn warm_library_pool_key(
    package: &str,
    version: &str,
    env: &HashMap<String, String>,
) -> String {
    let mut entries = env.iter().collect::<Vec<_>>();
    entries.sort_by(|a, b| a.0.cmp(b.0).then_with(|| a.1.cmp(b.1)));
    let env_key = entries
        .into_iter()
        .map(|(key, value)| format!("{}={}", key, value))
        .collect::<Vec<_>>()
        .join("\u{1f}");
    format!("{}@{}\u{1e}{}", package, version, env_key)
}

pub(super) fn proxy_library_invoke(
    stream: TcpStream,
    instance: Arc<WarmLibraryInstance>,
    invoke: &Message,
) -> Result<()> {
    {
        let mut writer = instance.writer.lock().unwrap();
        let json = serde_json::to_string(invoke)?;
        writeln!(writer, "{}", json)?;
        writer.flush()?;
    }

    let mut tcp_write = stream;
    let mut tcp_read = BufReader::new(tcp_write.try_clone()?);
    let done_signal = Arc::new(AtomicBool::new(false));
    let tcp_to_wasm = {
        let done_signal = done_signal.clone();
        let instance = instance.clone();
        std::thread::Builder::new()
            .name("zr-library-client-to-wasm".into())
            .spawn(move || {
                loop {
                    if done_signal.load(Ordering::Relaxed) {
                        break;
                    }
                    let mut line = String::new();
                    match tcp_read.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            let mut writer = instance.writer.lock().unwrap();
                            if writer.write_all(line.as_bytes()).is_err() || writer.flush().is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
            .ok()
    };

    loop {
        let line = {
            let mut reader = instance.reader.lock().unwrap();
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => line,
                Err(_) => break,
            }
        };

        if tcp_write.write_all(line.as_bytes()).is_err() || tcp_write.flush().is_err() {
            break;
        }
        if is_done_frame(&line) {
            break;
        }
    }

    done_signal.store(true, Ordering::Relaxed);
    let _ = tcp_write.shutdown(std::net::Shutdown::Both);
    if let Some(handle) = tcp_to_wasm {
        let _ = handle.join();
    }
    Ok(())
}

pub(super) fn release_warm_library_instance(instance: &WarmLibraryInstance) {
    *instance.last_used.lock().unwrap() = Instant::now();
    instance.busy.store(false, Ordering::Release);
}

pub(super) fn stop_warm_library_instance(instance: Arc<WarmLibraryInstance>) {
    if Arc::strong_count(&instance) != 1 {
        return;
    }

    if let Ok(instance) = Arc::try_unwrap(instance) {
        let WarmLibraryInstance {
            writer,
            reader,
            controller,
            ..
        } = instance;
        drop(writer);
        drop(reader);
        if let Some(controller) = controller.into_inner().unwrap().take() {
            let _ = controller.finish();
        }
    }
}
