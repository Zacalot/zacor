use crate::error::*;
use std::collections::HashMap;
use std::io::{BufReader, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::{Arc, Mutex};
use zacor_host::protocol::DaemonRefusal;
use zacor_package::protocol::{self as proto, Message};

use super::module_cache::{
    acquire_warm_library_instance, proxy_library_invoke, release_warm_library_instance, LibraryPool,
};
use super::{DaemonControl, DaemonRequest};

pub(super) fn handle_dispatch(
    mut reader: BufReader<TcpStream>,
    mut stream: TcpStream,
    req: DaemonRequest,
    control: &Arc<DaemonControl>,
    home: &Path,
) -> Result<()> {
    if let Some(refusal) = control.dispatch_refusal(req.zacor_version.as_deref()) {
        return send_ack_err(&mut stream, refusal);
    }

    let daemon_version = env!("CARGO_PKG_VERSION");
    if let Some(ref client_version) = req.zacor_version
        && client_version != daemon_version
    {
        let refusal = DaemonRefusal::VersionMismatch {
            daemon: daemon_version.into(),
            client: client_version.clone(),
        };
        control.begin_dispatch_drain(refusal.clone());
        let result = send_ack_err(
            &mut stream,
            refusal,
        );
        return result;
    }

    let pkg_name = match req.pkg_name {
        Some(n) => n,
        None => {
            return send_ack_err(
                &mut stream,
                DaemonRefusal::InvalidRequest {
                    reason: "dispatch: pkg_name required".into(),
                },
            )
        }
    };
    let version = match req.version {
        Some(v) => v,
        None => {
            return send_ack_err(
                &mut stream,
                DaemonRefusal::InvalidRequest {
                    reason: "dispatch: version required".into(),
                },
            )
        }
    };

    let def = match crate::wasm_manifest::load_from_store(home, &pkg_name, &version) {
        Ok(d) => d,
        Err(e) => {
            return send_ack_err(
                &mut stream,
                DaemonRefusal::LoadFailed {
                    reason: format!("load manifest for '{}' v{}: {:#}", pkg_name, version, e),
                },
            );
        }
    };

    let wasm_filename = match def.wasm.as_ref() {
        Some(w) => w,
        None => {
            return send_ack_err(
                &mut stream,
                DaemonRefusal::PackageNotFound {
                    name: pkg_name.clone(),
                },
            );
        }
    };

    let wasm_path = crate::paths::store_wasm_path(home, &pkg_name, &version, wasm_filename);
    if !wasm_path.exists() {
        return send_ack_err(
            &mut stream,
            DaemonRefusal::WasmArtifactMissing {
                path: wasm_path.display().to_string(),
            },
        );
    }

    let host = match crate::wasm_runtime::WasmHost::shared() {
        Ok(h) => h,
        Err(e) => {
            return send_ack_err(
                &mut stream,
                DaemonRefusal::LoadFailed {
                    reason: format!("wasm host: {:#}", e),
                },
            )
        }
    };
    let module = match host.load_module(&wasm_path) {
        Ok(m) => m,
        Err(e) => {
            return send_ack_err(
                &mut stream,
                DaemonRefusal::LoadFailed {
                    reason: format!("load {}: {:#}", wasm_path.display(), e),
                },
            );
        }
    };

    let env: Vec<(String, String)> = req.env.into_iter().collect();
    let session = match host.invoke(module, env) {
        Ok(s) => s,
        Err(e) => {
            return send_ack_err(
                &mut stream,
                DaemonRefusal::LoadFailed {
                    reason: format!("invoke: {:#}", e),
                },
            )
        }
    };

    writeln!(&mut stream, "{{\"ok\":true}}")?;
    stream.flush()?;

    let crate::wasm_runtime::WasmSession {
        writer: mut wasm_writer,
        reader: mut wasm_reader,
        controller,
    } = session;

    let shutdown_handle = stream.try_clone().context("cloning stream for shutdown handle")?;

    let tcp_to_wasm = std::thread::Builder::new()
        .name("zr-daemon-tcp-to-wasm".into())
        .spawn(move || {
            let _ = std::io::copy(&mut reader, &mut wasm_writer);
        })
        .context("spawning dispatch proxy thread")?;

    let _ = std::io::copy(&mut wasm_reader, &mut stream);
    let _ = shutdown_handle.shutdown(std::net::Shutdown::Both);
    let _ = tcp_to_wasm.join();
    let _ = controller.finish();

    Ok(())
}

pub(super) fn send_ack_err(stream: &mut TcpStream, refusal: DaemonRefusal) -> Result<()> {
    let error = refusal_message(&refusal);
    eprintln!("daemon: dispatch refused: {}", error);
    let ack = serde_json::json!({"ok": false, "refusal": refusal, "error": error});
    writeln!(stream, "{}", ack).context("writing dispatch err ack")?;
    stream.flush().context("flushing dispatch err ack")?;
    Ok(())
}

pub(super) fn handle_library_invoke(
    _reader: BufReader<TcpStream>,
    mut stream: TcpStream,
    req: DaemonRequest,
    library_pools: &Arc<Mutex<HashMap<String, LibraryPool>>>,
    control: &Arc<DaemonControl>,
    home: &Path,
) -> Result<()> {
    if let Some(refusal) = control.dispatch_refusal(req.zacor_version.as_deref()) {
        return send_ack_err(&mut stream, refusal);
    }

    let daemon_version = env!("CARGO_PKG_VERSION");
    if let Some(ref client_version) = req.zacor_version
        && client_version != daemon_version
    {
        let refusal = DaemonRefusal::VersionMismatch {
            daemon: daemon_version.into(),
            client: client_version.clone(),
        };
        control.begin_dispatch_drain(refusal.clone());
        let result = send_ack_err(
            &mut stream,
            refusal,
        );
        return result;
    }

    let pkg_name = match req.pkg_name {
        Some(name) => name,
        None => {
            return send_ack_err(
                &mut stream,
                DaemonRefusal::InvalidRequest {
                    reason: "invoke-library: pkg_name required".into(),
                },
            )
        }
    };
    let version = match req.version {
        Some(version) => version,
        None => {
            return send_ack_err(
                &mut stream,
                DaemonRefusal::InvalidRequest {
                    reason: "invoke-library: version required".into(),
                },
            )
        }
    };
    let command = match req.command {
        Some(command) => command,
        None => {
            return send_ack_err(
                &mut stream,
                DaemonRefusal::InvalidRequest {
                    reason: "invoke-library: command required".into(),
                },
            )
        }
    };

    let def = match crate::wasm_manifest::load_from_store(home, &pkg_name, &version) {
        Ok(def) => def,
        Err(error) => {
            return send_ack_err(
                &mut stream,
                DaemonRefusal::LoadFailed {
                    reason: format!("load manifest for '{}' v{}: {:#}", pkg_name, version, error),
                },
            )
        }
    };

    let service = match def.service.as_ref() {
        Some(service) if service.library => service,
        _ => {
            return send_ack_err(
                &mut stream,
                DaemonRefusal::InvalidRequest {
                    reason: format!("'{}' is not a library service package", pkg_name),
                },
            )
        }
    };

    let instance = match acquire_warm_library_instance(
        library_pools,
        home,
        &pkg_name,
        &version,
        &def,
        service,
        &req.env,
    ) {
        Ok(instance) => instance,
        Err(error) => {
            return send_ack_err(
                &mut stream,
                DaemonRefusal::LoadFailed {
                    reason: format!("warm library instance: {:#}", error),
                },
            )
        }
    };

    writeln!(&mut stream, "{{\"ok\":true}}")?;
    stream.flush()?;

    let invoke = Message::Invoke(proto::Invoke::from_str_args(command, &req.args, false));
    let result = proxy_library_invoke(stream, instance.clone(), &invoke);
    release_warm_library_instance(&instance);
    result
}

fn refusal_message(refusal: &DaemonRefusal) -> String {
    match refusal {
        DaemonRefusal::VersionMismatch { daemon, client } => {
            format!("daemon version mismatch: daemon={}, client={} - daemon will exit", daemon, client)
        }
        DaemonRefusal::PackageNotFound { name } => format!("package not found: {}", name),
        DaemonRefusal::WasmArtifactMissing { path } => format!("wasm artifact missing: {}", path),
        DaemonRefusal::LoadFailed { reason } => reason.clone(),
        DaemonRefusal::InvalidRequest { reason } => reason.clone(),
        DaemonRefusal::Other { message } => message.clone(),
    }
}
