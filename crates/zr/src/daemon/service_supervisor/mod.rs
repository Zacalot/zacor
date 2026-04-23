pub(super) mod subprocess;
pub(super) mod wasm;

use crate::error::*;
use crate::package_definition::PackageDefinition;
use crate::{paths, receipt};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use super::{DaemonResponse, ServiceStatusEntry, HEALTH_CHECK_TIMEOUT};
use subprocess::wait_for_health;

use wasm::WasmServiceHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ServiceState {
    Running,
    Failed,
}

impl std::fmt::Display for ServiceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceState::Running => write!(f, "running"),
            ServiceState::Failed => write!(f, "failed"),
        }
    }
}

pub(super) struct ManagedService {
    pub(super) name: String,
    pub(super) port: u16,
    pub(super) health_path: String,
    pub(super) state: ServiceState,
    pub(super) consecutive_failures: u32,
    pub(super) kind: ManagedKind,
}

pub(super) enum ManagedKind {
    Subprocess(SubprocessService),
    Wasm(Box<WasmServiceHandle>),
}

pub(super) struct SubprocessService {
    pub(super) process: Option<Child>,
    pub(super) bin_path: PathBuf,
}

pub(super) fn handle_status(
    services: &Arc<Mutex<HashMap<String, ManagedService>>>,
) -> DaemonResponse {
    let services = services.lock().unwrap();
    let entries: Vec<ServiceStatusEntry> = services
        .values()
        .map(|svc| ServiceStatusEntry {
            name: svc.name.clone(),
            port: svc.port,
            status: svc.state.to_string(),
        })
        .collect();
    DaemonResponse {
        ok: true,
        error: None,
        refusal: None,
        services: Some(entries),
        ..Default::default()
    }
}

pub(super) fn handle_start_service(
    services: &Arc<Mutex<HashMap<String, ManagedService>>>,
    home: &Path,
    name: &str,
) -> DaemonResponse {
    {
        let svcs = services.lock().unwrap();
        if let Some(svc) = svcs.get(name)
            && svc.state == ServiceState::Running
        {
            return DaemonResponse {
                ok: true,
                error: None,
                refusal: None,
                services: None,
                ..Default::default()
            };
        }
    }

    let (def, version) = match load_package_def(home, name) {
        Ok(v) => v,
        Err(e) => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("{:#}", e)),
                refusal: None,
                services: None,
                ..Default::default()
            };
        }
    };

    let service = match def.service.as_ref() {
        Some(s) => s,
        None => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("package '{}' has no service section", name)),
                refusal: None,
                services: None,
                ..Default::default()
            };
        }
    };

    let port = match service.port {
        Some(p) => p,
        None => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("service '{}' has no port configured", name)),
                refusal: None,
                services: None,
                ..Default::default()
            };
        }
    };

    let health_path = service.health.clone().unwrap_or_else(|| "/health".into());

    if def.wasm.is_some() {
        return wasm::start_wasm_service(services, home, name, &version, &def, port, health_path);
    }

    let binary_name = match def.binary.as_ref() {
        Some(b) => b,
        None => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("package '{}' has no binary", name)),
                refusal: None,
                services: None,
                ..Default::default()
            };
        }
    };

    let bin_path = paths::store_binary_path(home, name, &version, binary_name);
    if !bin_path.exists() {
        return DaemonResponse {
            ok: false,
            error: Some(format!("binary not found: {}", bin_path.display())),
            refusal: None,
            services: None,
            ..Default::default()
        };
    }

    let child = match Command::new(&bin_path)
        .arg(format!("--listen=:{}", port))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("failed to spawn service: {}", e)),
                refusal: None,
                services: None,
                ..Default::default()
            };
        }
    };

    eprintln!("daemon: started service '{}' on port {}", name, port);

    if !wait_for_health(port, &health_path, HEALTH_CHECK_TIMEOUT) {
        return DaemonResponse {
            ok: false,
            error: Some("health check timeout".into()),
            refusal: None,
            services: None,
            ..Default::default()
        };
    }

    let mut svcs = services.lock().unwrap();
    svcs.insert(
        name.to_string(),
        ManagedService {
            name: name.to_string(),
            port,
            health_path,
            state: ServiceState::Running,
            consecutive_failures: 0,
            kind: ManagedKind::Subprocess(SubprocessService {
                process: Some(child),
                bin_path,
            }),
        },
    );

    DaemonResponse {
        ok: true,
        error: None,
        refusal: None,
        services: None,
        ..Default::default()
    }
}

pub(super) fn stop_managed(svc: ManagedService) {
    match svc.kind {
        ManagedKind::Subprocess(mut sub) => {
            if let Some(ref mut child) = sub.process {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        ManagedKind::Wasm(mut handle) => {
            handle.shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = std::net::TcpStream::connect(format!("127.0.0.1:{}", svc.port));
            if let Some(t) = handle.accept_thread.take() {
                let _ = t.join();
            }
        }
    }
}

pub(super) fn handle_stop_service(
    services: &Arc<Mutex<HashMap<String, ManagedService>>>,
    name: &str,
) -> DaemonResponse {
    let removed = {
        let mut svcs = services.lock().unwrap();
        svcs.remove(name)
    };
    if let Some(svc) = removed {
        eprintln!("daemon: stopping service '{}'", name);
        stop_managed(svc);
    }
    DaemonResponse {
        ok: true,
        error: None,
        refusal: None,
        services: None,
        ..Default::default()
    }
}

fn load_package_def(home: &Path, name: &str) -> Result<(PackageDefinition, String)> {
    let receipt = receipt::read(home, name)?.ok_or_else(|| anyhow!("package '{}' not found", name))?;
    let version = receipt.current.clone();
    let def = crate::wasm_manifest::load_from_store(home, name, &version)?;
    Ok((def, version))
}
