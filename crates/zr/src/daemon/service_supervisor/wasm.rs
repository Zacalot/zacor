use crate::error::*;
use crate::package_definition::PackageDefinition;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::{ManagedKind, ManagedService, ServiceState};
use crate::daemon::DaemonResponse;

pub(in crate::daemon) struct WasmServiceHandle {
    #[allow(dead_code)]
    writer: Arc<Mutex<crate::wasm_runtime::BridgedWriter>>,
    #[allow(dead_code)]
    reader: Arc<Mutex<BufReader<crate::wasm_runtime::BridgedReader>>>,
    #[allow(dead_code)]
    conn_lock: Arc<Mutex<()>>,
    pub(super) shutdown: Arc<AtomicBool>,
    pub(super) accept_thread: Option<std::thread::JoinHandle<()>>,
}

pub(in crate::daemon) fn start_wasm_service(
    services: &Arc<Mutex<HashMap<String, ManagedService>>>,
    home: &Path,
    name: &str,
    version: &str,
    def: &PackageDefinition,
    port: u16,
    health_path: String,
) -> DaemonResponse {
    let wasm_filename = match def.wasm.as_ref() {
        Some(w) => w,
        None => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("package '{}' has no wasm artifact", name)),
                refusal: None,
                services: None,
                ..Default::default()
            };
        }
    };

    let wasm_path = crate::paths::store_wasm_path(home, name, version, wasm_filename);
    if !wasm_path.exists() {
        return DaemonResponse {
            ok: false,
            error: Some(format!("wasm artifact missing: {}", wasm_path.display())),
            refusal: None,
            services: None,
            ..Default::default()
        };
    }

    let host = match crate::wasm_runtime::WasmHost::shared() {
        Ok(h) => h,
        Err(e) => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("wasm host: {:#}", e)),
                refusal: None,
                services: None,
                ..Default::default()
            };
        }
    };
    let module = match host.load_module(&wasm_path) {
        Ok(m) => m,
        Err(e) => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("load {}: {:#}", wasm_path.display(), e)),
                refusal: None,
                services: None,
                ..Default::default()
            };
        }
    };

    let session = match host.invoke(module, Vec::new()) {
        Ok(s) => s,
        Err(e) => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("wasm invoke: {:#}", e)),
                refusal: None,
                services: None,
                ..Default::default()
            };
        }
    };

    let crate::wasm_runtime::WasmSession {
        writer,
        reader,
        controller,
    } = session;
    drop(controller);

    let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
        Ok(l) => l,
        Err(e) => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("failed to bind service port {}: {}", port, e)),
                refusal: None,
                services: None,
                ..Default::default()
            };
        }
    };

    let writer_arc = Arc::new(Mutex::new(writer));
    let reader_arc = Arc::new(Mutex::new(reader));
    let conn_lock = Arc::new(Mutex::new(()));
    let shutdown = Arc::new(AtomicBool::new(false));

    let accept_thread = {
        let writer = writer_arc.clone();
        let reader = reader_arc.clone();
        let conn_lock = conn_lock.clone();
        let shutdown = shutdown.clone();
        let health_path = health_path.clone();
        let svc_name = name.to_string();
        std::thread::Builder::new()
            .name(format!("zr-wasm-svc-{}", name))
            .spawn(move || {
                wasm_service_accept_loop(
                    listener,
                    writer,
                    reader,
                    conn_lock,
                    shutdown,
                    svc_name,
                    health_path,
                );
            })
            .ok()
    };

    eprintln!("daemon: started wasm service '{}' on port {}", name, port);

    let mut svcs = services.lock().unwrap();
    svcs.insert(
        name.to_string(),
        ManagedService {
            name: name.to_string(),
            port,
            health_path,
            state: ServiceState::Running,
            consecutive_failures: 0,
            kind: ManagedKind::Wasm(Box::new(WasmServiceHandle {
                writer: writer_arc,
                reader: reader_arc,
                conn_lock,
                shutdown,
                accept_thread,
            })),
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

pub(in crate::daemon) fn wasm_service_accept_loop(
    listener: TcpListener,
    writer: Arc<Mutex<crate::wasm_runtime::BridgedWriter>>,
    reader: Arc<Mutex<BufReader<crate::wasm_runtime::BridgedReader>>>,
    conn_lock: Arc<Mutex<()>>,
    shutdown: Arc<AtomicBool>,
    svc_name: String,
    health_path: String,
) {
    for stream in listener.incoming() {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        let stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };

        let writer = writer.clone();
        let reader = reader.clone();
        let conn_lock = conn_lock.clone();
        let svc_name = svc_name.clone();
        let health_path = health_path.clone();
        std::thread::Builder::new()
            .name(format!("zr-wasm-svc-conn-{}", svc_name))
            .spawn(move || {
                if let Err(e) =
                    handle_wasm_service_connection(stream, writer, reader, conn_lock, &health_path)
                {
                    eprintln!("daemon: wasm service '{}' conn error: {:#}", svc_name, e);
                }
            })
            .ok();
    }
}

fn handle_wasm_service_connection(
    stream: TcpStream,
    writer: Arc<Mutex<crate::wasm_runtime::BridgedWriter>>,
    reader: Arc<Mutex<BufReader<crate::wasm_runtime::BridgedReader>>>,
    conn_lock: Arc<Mutex<()>>,
    health_path: &str,
) -> Result<()> {
    let mut peek_reader = BufReader::new(stream.try_clone()?);
    let mut first_line = String::new();
    if peek_reader.read_line(&mut first_line)? == 0 {
        return Ok(());
    }

    if first_line.starts_with("GET ") || first_line.starts_with("HEAD ") {
        let _ = health_path;
        loop {
            let mut header = String::new();
            if peek_reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
                break;
            }
        }
        let body = r#"{"status":"ok"}"#;
        let response = format!(
            "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let mut out = stream;
        out.write_all(response.as_bytes())?;
        out.flush()?;
        return Ok(());
    }

    let _conn = conn_lock.lock().unwrap();

    let mut tcp_write = stream;
    {
        let mut w = writer.lock().unwrap();
        w.write_all(first_line.as_bytes())?;
        w.flush()?;
    }

    let done_signal = Arc::new(AtomicBool::new(false));
    let tcp_to_wasm = {
        let done_signal = done_signal.clone();
        let writer = writer.clone();
        std::thread::Builder::new()
            .name("zr-wasm-svc-tcp-to-wasm".into())
            .spawn(move || {
                let mut reader = peek_reader;
                loop {
                    if done_signal.load(Ordering::Relaxed) {
                        break;
                    }
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            let mut w = writer.lock().unwrap();
                            if w.write_all(line.as_bytes()).is_err() {
                                break;
                            }
                            if w.flush().is_err() {
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
            let mut r = reader.lock().unwrap();
            let mut line = String::new();
            match r.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => line,
                Err(_) => break,
            }
        };
        if tcp_write.write_all(line.as_bytes()).is_err() {
            break;
        }
        if tcp_write.flush().is_err() {
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

pub(in crate::daemon) fn is_done_frame(line: &str) -> bool {
    line.contains("\"type\":\"done\"") || line.contains("\"type\": \"done\"")
}
