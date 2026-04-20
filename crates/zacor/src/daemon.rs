use crate::error::*;
use crate::package_definition::PackageDefinition;
use crate::paths;
use crate::receipt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DAEMON_PORT: u16 = 19100;
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(10);
const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESTART_FAILURES: u32 = 5;
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// Default self-exit threshold with no client activity (and no managed
/// services). Overridable via `ZR_DAEMON_IDLE_TIMEOUT_SECS`; setting the
/// env var to `0` disables self-exit entirely.
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 30 * 60;

/// Resolve the idle-timeout from the environment at daemon start.
/// Returns `None` when the operator has disabled self-exit.
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum ServiceState {
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

struct ManagedService {
    name: String,
    port: u16,
    health_path: String,
    state: ServiceState,
    consecutive_failures: u32,
    kind: ManagedKind,
}

enum ManagedKind {
    /// Native subprocess holding its own TCP listener on `service.port`.
    /// Daemon only supervises the process lifecycle.
    Subprocess(SubprocessService),
    /// Wasm module running inside the daemon as a long-lived instance.
    /// Daemon binds the TCP listener and proxies INVOKE/OUTPUT between
    /// clients and the wasm stdio pipes. `Store: !Send` forces all wasm
    /// access to be single-threaded; `conn_lock` serializes connections.
    Wasm(Box<WasmServiceHandle>),
}

struct SubprocessService {
    process: Option<Child>,
    bin_path: PathBuf,
}

struct WasmServiceHandle {
    // Wasm stdin / stdout under separate mutexes so the per-connection
    // TCP→wasm and wasm→TCP threads can run concurrently without
    // deadlocking. Inter-connection serialization is `conn_lock`. The
    // fields are "owning references" — their value is never read
    // directly on this handle, but dropping them closes the wasm pipes.
    #[allow(dead_code)]
    writer: Arc<Mutex<crate::wasm_runtime::BridgedWriter>>,
    #[allow(dead_code)]
    reader: Arc<Mutex<BufReader<crate::wasm_runtime::BridgedReader>>>,
    #[allow(dead_code)]
    conn_lock: Arc<Mutex<()>>,
    shutdown: Arc<AtomicBool>,
    accept_thread: Option<std::thread::JoinHandle<()>>,
}

pub struct DaemonServer {
    home: PathBuf,
    services: Arc<Mutex<HashMap<String, ManagedService>>>,
    shutdown: Arc<Mutex<bool>>,
    last_activity: Arc<Mutex<Instant>>,
}

#[derive(Debug, Deserialize)]
struct DaemonRequest {
    request: String,
    #[serde(default)]
    name: Option<String>,
    // Dispatch-specific fields — optional for other request types.
    #[serde(default)]
    pkg_name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    /// Client's zacor version — used on dispatch to detect daemon/client
    /// version skew. When present and mismatched, the daemon refuses the
    /// request and self-exits so a fresh daemon can start.
    #[serde(default)]
    zacor_version: Option<String>,
}

#[derive(Debug, Serialize)]
struct DaemonResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    services: Option<Vec<ServiceStatusEntry>>,
}

#[derive(Debug, Serialize)]
struct ServiceStatusEntry {
    name: String,
    port: u16,
    status: String,
}

impl DaemonServer {
    pub fn new(home: PathBuf) -> Self {
        DaemonServer {
            home,
            services: Arc::new(Mutex::new(HashMap::new())),
            shutdown: Arc::new(Mutex::new(false)),
            last_activity: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// Run the daemon: IPC listener + health monitor loop.
    pub fn run(&self) -> Result<()> {
        let addr = format!("127.0.0.1:{}", DAEMON_PORT);
        let listener = TcpListener::bind(&addr).with_context(|| {
            format!(
                "failed to bind daemon on {} — is another daemon running?",
                addr
            )
        })?;

        eprintln!("daemon: listening on {}", addr);

        // Set a timeout on the listener so we can check for shutdown
        listener.set_nonblocking(false).ok();

        // Start health monitor thread
        let services = self.services.clone();
        let shutdown = self.shutdown.clone();
        let last_activity = self.last_activity.clone();
        std::thread::Builder::new()
            .name("zr-health-monitor".into())
            .spawn(move || health_monitor_loop(services, shutdown, last_activity))
            .context("failed to spawn health monitor")?;

        // Accept IPC connections
        for stream in listener.incoming() {
            if *self.shutdown.lock().unwrap() {
                break;
            }
            match stream {
                Ok(stream) => {
                    let services = self.services.clone();
                    let shutdown = self.shutdown.clone();
                    let last_activity = self.last_activity.clone();
                    let home = self.home.clone();
                    std::thread::Builder::new()
                        .name("zr-daemon-conn".into())
                        .spawn(move || {
                            if let Err(e) = handle_connection(
                                stream,
                                &services,
                                &shutdown,
                                &last_activity,
                                &home,
                            ) {
                                eprintln!("daemon: connection error: {:#}", e);
                            }
                        })
                        .ok();
                }
                Err(e) => {
                    if *self.shutdown.lock().unwrap() {
                        break;
                    }
                    eprintln!("daemon: accept error: {}", e);
                }
            }
        }

        // Shutdown: stop all services
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
    }
}

fn handle_connection(
    stream: TcpStream,
    services: &Arc<Mutex<HashMap<String, ManagedService>>>,
    shutdown: &Arc<Mutex<bool>>,
    last_activity: &Arc<Mutex<Instant>>,
    home: &Path,
) -> Result<()> {
    // Any client connection counts as activity; resets idle-timeout clock.
    *last_activity.lock().unwrap() = Instant::now();

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let req: DaemonRequest = serde_json::from_str(line.trim()).context("invalid daemon request")?;

    // Dispatch takes over the connection for bidirectional wasm-protocol
    // streaming — no single DaemonResponse is written. Branch early.
    if req.request == "dispatch" {
        return handle_dispatch(reader, stream, req, shutdown, home);
    }

    let response = match req.request.as_str() {
        "ping" => DaemonResponse {
            ok: true,
            error: None,
            services: None,
        },
        "status" => handle_status(services),
        "start-service" => {
            let name = req.name.as_deref().unwrap_or("");
            if name.is_empty() {
                DaemonResponse {
                    ok: false,
                    error: Some("missing service name".into()),
                    services: None,
                }
            } else {
                handle_start_service(services, home, name)
            }
        }
        "stop-service" => {
            let name = req.name.as_deref().unwrap_or("");
            handle_stop_service(services, name)
        }
        "shutdown" => {
            *shutdown.lock().unwrap() = true;
            // Connect to ourselves to unblock the accept loop
            let _ = TcpStream::connect(format!("127.0.0.1:{}", DAEMON_PORT));
            DaemonResponse {
                ok: true,
                error: None,
                services: None,
            }
        }
        _ => DaemonResponse {
            ok: false,
            error: Some(format!("unknown request: {}", req.request)),
            services: None,
        },
    };

    let mut writer = stream;
    let json = serde_json::to_string(&response)?;
    writeln!(writer, "{}", json)?;
    writer.flush()?;
    Ok(())
}

/// Handle a `dispatch` RPC. The daemon:
/// 1. Re-resolves the wasm artifact from (pkg_name, version) using its
///    own ZR_HOME (never trusts client-supplied paths).
/// 2. Loads the module via the process-wide `WasmHost` — subsequent
///    dispatches of the same package hit the in-memory cache.
/// 3. Spawns a wasm session with the client-supplied env vars.
/// 4. Acks, then becomes a transparent bidirectional byte proxy
///    between the TCP connection and the wasm's stdio pipes. The
///    client's `run_protocol_session` drives the wasm protocol
///    end-to-end; capability handling happens client-side so
///    filesystem paths resolve against the user's actual cwd.
fn handle_dispatch(
    mut reader: BufReader<TcpStream>,
    mut stream: TcpStream,
    req: DaemonRequest,
    shutdown: &Arc<Mutex<bool>>,
    home: &Path,
) -> Result<()> {
    // Version handshake: if the client reports a zacor version different
    // from the daemon's own, the daemon is stale relative to this client.
    // Refuse the dispatch and self-exit so a fresh daemon can start.
    let daemon_version = env!("CARGO_PKG_VERSION");
    if let Some(ref client_version) = req.zacor_version
        && client_version != daemon_version
    {
        let result = send_ack_err(
            &mut stream,
            format!(
                "daemon version mismatch: daemon={}, client={} — daemon will exit",
                daemon_version, client_version
            ),
        );
        // Trigger self-exit AFTER the ack has been flushed, so the client
        // reliably sees the reason before the TCP connection tears down.
        *shutdown.lock().unwrap() = true;
        let _ = TcpStream::connect(format!("127.0.0.1:{}", DAEMON_PORT));
        return result;
    }

    let pkg_name = match req.pkg_name {
        Some(n) => n,
        None => return send_ack_err(&mut stream, "dispatch: pkg_name required".into()),
    };
    let version = match req.version {
        Some(v) => v,
        None => return send_ack_err(&mut stream, "dispatch: version required".into()),
    };

    // Re-resolve wasm path from the daemon's own view of the store.
    // Don't trust client-supplied paths — the daemon is the authority.
    let def = match crate::wasm_manifest::load_from_store(home, &pkg_name, &version) {
        Ok(d) => d,
        Err(e) => {
            return send_ack_err(
                &mut stream,
                format!("load manifest for '{}' v{}: {:#}", pkg_name, version, e),
            );
        }
    };

    let wasm_filename = match def.wasm.as_ref() {
        Some(w) => w,
        None => {
            return send_ack_err(
                &mut stream,
                format!("'{}' is not a wasm package (no `wasm:` field)", pkg_name),
            );
        }
    };

    let wasm_path = crate::paths::store_wasm_path(home, &pkg_name, &version, wasm_filename);
    if !wasm_path.exists() {
        return send_ack_err(
            &mut stream,
            format!("wasm artifact missing: {}", wasm_path.display()),
        );
    }

    // Load module — hot cache hit after first dispatch.
    let host = match crate::wasm_runtime::WasmHost::shared() {
        Ok(h) => h,
        Err(e) => return send_ack_err(&mut stream, format!("wasm host: {:#}", e)),
    };
    let module = match host.load_module(&wasm_path) {
        Ok(m) => m,
        Err(e) => {
            return send_ack_err(&mut stream, format!("load {}: {:#}", wasm_path.display(), e));
        }
    };

    let env: Vec<(String, String)> = req.env.into_iter().collect();
    let session = match host.invoke(module, env) {
        Ok(s) => s,
        Err(e) => return send_ack_err(&mut stream, format!("invoke: {:#}", e)),
    };

    // Ack — after this, the connection is a wasm-protocol byte pipe.
    writeln!(&mut stream, "{{\"ok\":true}}").context("writing dispatch ack")?;
    stream.flush().context("flushing dispatch ack")?;

    let crate::wasm_runtime::WasmSession {
        writer: mut wasm_writer,
        reader: mut wasm_reader,
        controller,
    } = session;

    // We need a second handle to the TCP stream so one half can stay in
    // the main thread (for wasm → TCP) while the other goes into a
    // spawned thread (for TCP → wasm). We also want a shutdown handle
    // to unblock the spawned thread's blocking read when wasm exits.
    let shutdown_handle = stream
        .try_clone()
        .context("cloning stream for shutdown handle")?;

    let tcp_to_wasm = std::thread::Builder::new()
        .name("zr-daemon-tcp-to-wasm".into())
        .spawn(move || {
            // TCP → wasm stdin. EOFs on TCP close; error on broken pipe.
            let _ = std::io::copy(&mut reader, &mut wasm_writer);
        })
        .context("spawning dispatch proxy thread")?;

    // Wasm stdout → TCP. Returns when wasm exits (stdout closes) or
    // when the TCP write fails (client disconnected).
    let _ = std::io::copy(&mut wasm_reader, &mut stream);

    // Wasm output ended. Shut down the TCP connection to unblock the
    // other thread's blocking read on the client side.
    let _ = shutdown_handle.shutdown(std::net::Shutdown::Both);

    // Wait for the proxy thread to drain / exit.
    let _ = tcp_to_wasm.join();

    // Join the wasm thread — surfaces any trap captured during `_start`.
    // Ignore the result: by the time we get here either the module ran
    // to DONE (`proc_exit(0)` is a "normal" trap) or the client dropped.
    let _ = controller.finish();

    Ok(())
}

fn send_ack_err(stream: &mut TcpStream, error: String) -> Result<()> {
    eprintln!("daemon: dispatch refused: {}", error);
    let ack = serde_json::json!({"ok": false, "error": error});
    writeln!(stream, "{}", ack).context("writing dispatch err ack")?;
    stream.flush().context("flushing dispatch err ack")?;
    Ok(())
}

fn handle_status(services: &Arc<Mutex<HashMap<String, ManagedService>>>) -> DaemonResponse {
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
        services: Some(entries),
    }
}

fn handle_start_service(
    services: &Arc<Mutex<HashMap<String, ManagedService>>>,
    home: &Path,
    name: &str,
) -> DaemonResponse {
    // Check if already running
    {
        let svcs = services.lock().unwrap();
        if let Some(svc) = svcs.get(name) {
            if svc.state == ServiceState::Running {
                return DaemonResponse {
                    ok: true,
                    error: None,
                    services: None,
                };
            }
        }
    }

    // Load package definition to get service config
    let (def, version) = match load_package_def(home, name) {
        Ok(v) => v,
        Err(e) => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("{:#}", e)),
                services: None,
            };
        }
    };

    let service = match def.service.as_ref() {
        Some(s) => s,
        None => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("package '{}' has no service section", name)),
                services: None,
            };
        }
    };

    let port = match service.port {
        Some(p) => p,
        None => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("service '{}' has no port configured", name)),
                services: None,
            };
        }
    };

    let health_path = service.health.clone().unwrap_or_else(|| "/health".into());

    // Wasm-hosted service: daemon binds the TCP listener and drives the
    // wasm instance in-process. `service.start` is ignored — it is a
    // native-subprocess concept with no meaning here.
    if def.wasm.is_some() {
        return start_wasm_service(services, home, name, &version, &def, port, health_path);
    }

    let binary_name = match def.binary.as_ref() {
        Some(b) => b,
        None => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("package '{}' has no binary", name)),
                services: None,
            };
        }
    };

    let bin_path = paths::store_binary_path(home, name, &version, binary_name);
    if !bin_path.exists() {
        return DaemonResponse {
            ok: false,
            error: Some(format!("binary not found: {}", bin_path.display())),
            services: None,
        };
    }

    // Spawn the service
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
                services: None,
            };
        }
    };

    eprintln!("daemon: started service '{}' on port {}", name, port);

    // Wait for health check to pass
    if !wait_for_health(port, &health_path, HEALTH_CHECK_TIMEOUT) {
        return DaemonResponse {
            ok: false,
            error: Some("health check timeout".into()),
            services: None,
        };
    }

    let mut svcs = services.lock().unwrap();
    // Reset failure count on manual start
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
        services: None,
    }
}

/// Start a wasm-hosted service. The daemon:
/// 1. Loads the wasm module and calls `WasmHost::invoke` — the wasm
///    thread begins running, blocks on its first stdin read inside
///    `service_loop_stdin`.
/// 2. Binds `127.0.0.1:<port>` as the service listener.
/// 3. Spawns an accept thread that serializes TCP connections through
///    the pinned wasm instance.
///
/// Health checks for wasm services are answered by the daemon directly
/// — the wasm instance never sees HTTP traffic.
fn start_wasm_service(
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
                services: None,
            };
        }
    };

    let wasm_path = paths::store_wasm_path(home, name, version, wasm_filename);
    if !wasm_path.exists() {
        return DaemonResponse {
            ok: false,
            error: Some(format!("wasm artifact missing: {}", wasm_path.display())),
            services: None,
        };
    }

    let host = match crate::wasm_runtime::WasmHost::shared() {
        Ok(h) => h,
        Err(e) => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("wasm host: {:#}", e)),
                services: None,
            };
        }
    };
    let module = match host.load_module(&wasm_path) {
        Ok(m) => m,
        Err(e) => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("load {}: {:#}", wasm_path.display(), e)),
                services: None,
            };
        }
    };

    // Spawn the wasm instance — blocks in `service_loop_stdin` on its
    // first stdin read. The instance stays alive until stdin closes.
    let session = match host.invoke(module, Vec::new()) {
        Ok(s) => s,
        Err(e) => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("wasm invoke: {:#}", e)),
                services: None,
            };
        }
    };

    let crate::wasm_runtime::WasmSession {
        writer,
        reader,
        controller,
    } = session;
    // Controller's JoinHandle is detached — on shutdown we close stdin
    // (by dropping `writer`) and let the wasm thread exit on its own.
    drop(controller);

    let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
        Ok(l) => l,
        Err(e) => {
            return DaemonResponse {
                ok: false,
                error: Some(format!("failed to bind service port {}: {}", port, e)),
                services: None,
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
        services: None,
    }
}

/// Accept loop for a wasm-hosted service. Serializes TCP connections
/// through a single mutex so the non-Send wasm instance sees exactly
/// one INVOKE at a time.
fn wasm_service_accept_loop(
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

/// Handle one connection to a wasm-hosted service.
///
/// Peeks the first line to detect HTTP health checks (answered directly
/// by the daemon) vs protocol INVOKE. For protocol connections, acquires
/// the service's connection lock (only one active INVOKE at a time),
/// then proxies lines bidirectionally between the TCP client and the
/// pinned wasm instance. Terminates this TCP session on a DONE line from
/// wasm — the wasm instance stays alive for the next connection.
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

    // HTTP health-check short-circuit — daemon answers without touching
    // the wasm instance.
    if first_line.starts_with("GET ") || first_line.starts_with("HEAD ") {
        let _ = health_path; // path distinction not needed; all paths 200
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

    // Protocol connection — serialize against other clients.
    let _conn = conn_lock.lock().unwrap();

    let mut tcp_write = stream;

    // Forward the INVOKE line we already peeked.
    {
        let mut w = writer.lock().unwrap();
        w.write_all(first_line.as_bytes())?;
        w.flush()?;
    }

    // TCP → wasm stdin (INPUT / CAPABILITY_RES lines from client).
    // `peek_reader` already wraps a cloned read handle of the TCP stream;
    // move it into this thread for the rest of the connection.
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

    // Wasm stdout → TCP; break on DONE to close this session while
    // leaving the wasm instance alive for the next connection.
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

    // Tell the TCP→wasm thread to stop; close TCP read half to unblock
    // its `read_line` if it's still blocked.
    done_signal.store(true, Ordering::Relaxed);
    let _ = tcp_write.shutdown(std::net::Shutdown::Both);
    if let Some(handle) = tcp_to_wasm {
        let _ = handle.join();
    }

    Ok(())
}

/// Quick DONE-frame detection. A well-formed DONE line is a JSON object
/// containing `"type":"done"`. String search is sufficient because the
/// protocol spec forbids nested JSON strings matching that exact token.
fn is_done_frame(line: &str) -> bool {
    line.contains("\"type\":\"done\"") || line.contains("\"type\": \"done\"")
}

/// Drive a managed service to a stop and release all its resources.
/// For subprocesses: kill + wait. For wasm: signal shutdown, unblock
/// the accept thread with a self-connect, drop the writer (closes the
/// wasm instance's stdin → clean exit).
fn stop_managed(svc: ManagedService) {
    match svc.kind {
        ManagedKind::Subprocess(mut sub) => {
            if let Some(ref mut child) = sub.process {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        ManagedKind::Wasm(mut handle) => {
            handle.shutdown.store(true, Ordering::Relaxed);
            // Self-connect to unblock the accept loop. Best-effort —
            // if the listener is already closed the connect just fails.
            let _ = TcpStream::connect(format!("127.0.0.1:{}", svc.port));
            if let Some(t) = handle.accept_thread.take() {
                let _ = t.join();
            }
            // Dropping `handle` (including `writer`) closes the wasm
            // instance's stdin. Its `service_loop_stdin` sees EOF and
            // returns; the detached wasm thread exits cleanly.
        }
    }
}

fn handle_stop_service(
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
    // Idempotent: ok even if not running
    DaemonResponse {
        ok: true,
        error: None,
        services: None,
    }
}

fn load_package_def(home: &Path, name: &str) -> Result<(PackageDefinition, String)> {
    let receipt =
        receipt::read(home, name)?.ok_or_else(|| anyhow!("package '{}' not found", name))?;
    let version = receipt.current.clone();
    let def = crate::wasm_manifest::load_from_store(home, name, &version)?;
    Ok((def, version))
}

/// Wait for a service health endpoint to respond with HTTP 200.
fn wait_for_health(port: u16, health_path: &str, timeout: Duration) -> bool {
    let start = Instant::now();
    let interval = Duration::from_millis(100);

    loop {
        if check_health(port, health_path) {
            return true;
        }
        if start.elapsed() > timeout {
            return false;
        }
        std::thread::sleep(interval);
    }
}

/// Check health of a service via a simple HTTP GET.
fn check_health(port: u16, health_path: &str) -> bool {
    let addr = format!("127.0.0.1:{}", port);
    let Ok(mut stream) = TcpStream::connect(&addr) else {
        return false;
    };
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    let request = format!(
        "GET {} HTTP/1.0\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        health_path, port
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    if stream.flush().is_err() {
        return false;
    }
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    if reader.read_line(&mut status_line).is_err() {
        return false;
    }
    // Check for HTTP 200
    status_line.contains("200")
}

/// Calculate exponential backoff duration for restart attempts.
/// Sequence: 1s, 2s, 4s, 8s, then capped at 60s.
fn backoff_duration(failures: u32) -> Duration {
    let shift = failures.saturating_sub(1).min(63);
    let secs = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
    Duration::from_secs(secs.min(MAX_BACKOFF.as_secs()))
}

/// Health monitor loop: checks each service periodically, restarts on failure.
/// Also checks the idle-timeout: when no services are running and no client
/// activity for `IDLE_TIMEOUT`, trips `shutdown` so the daemon self-exits.
fn health_monitor_loop(
    services: Arc<Mutex<HashMap<String, ManagedService>>>,
    shutdown: Arc<Mutex<bool>>,
    last_activity: Arc<Mutex<Instant>>,
) {
    loop {
        std::thread::sleep(HEALTH_CHECK_INTERVAL);

        if *shutdown.lock().unwrap() {
            break;
        }

        let names: Vec<String> = {
            let svcs = services.lock().unwrap();
            svcs.keys().cloned().collect()
        };

        // Idle self-exit: only when no managed services are running and
        // the operator hasn't disabled it via ZR_DAEMON_IDLE_TIMEOUT_SECS=0.
        if names.is_empty()
            && let Some(timeout) = idle_timeout()
        {
            let idle = last_activity.lock().unwrap().elapsed();
            if idle >= timeout {
                eprintln!(
                    "daemon: idle for {:?} with no services — exiting",
                    idle
                );
                *shutdown.lock().unwrap() = true;
                let _ = TcpStream::connect(format!("127.0.0.1:{}", DAEMON_PORT));
                return;
            }
        }

        for name in names {
            if *shutdown.lock().unwrap() {
                return;
            }

            let mut svcs = services.lock().unwrap();
            let Some(svc) = svcs.get_mut(&name) else {
                continue;
            };

            if svc.state == ServiceState::Failed {
                continue;
            }

            // Wasm-hosted services: daemon owns the listener and answers
            // /health directly, so the HTTP probe would just reach
            // ourselves. Skip restart logic — a trapped wasm instance
            // needs a full re-invoke that this loop doesn't perform.
            let sub = match &mut svc.kind {
                ManagedKind::Subprocess(s) => s,
                ManagedKind::Wasm(_) => continue,
            };

            // Check if process is still alive
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

            // Unhealthy or dead — attempt restart
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

            // Kill old process
            if let Some(ref mut child) = sub.process {
                let _ = child.kill();
                let _ = child.wait();
            }

            let wait = backoff_duration(svc.consecutive_failures);
            let bin_path = sub.bin_path.clone();
            let port = svc.port;
            let health_path = svc.health_path.clone();
            let svc_name = name.clone();

            // Drop the lock before sleeping
            drop(svcs);

            eprintln!("daemon: restarting '{}' in {:?}", svc_name, wait);
            std::thread::sleep(wait);

            if *shutdown.lock().unwrap() {
                return;
            }

            // Respawn
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn test_backoff_duration() {
        assert_eq!(backoff_duration(1), Duration::from_secs(1));
        assert_eq!(backoff_duration(2), Duration::from_secs(2));
        assert_eq!(backoff_duration(3), Duration::from_secs(4));
        assert_eq!(backoff_duration(4), Duration::from_secs(8));
        assert_eq!(backoff_duration(5), Duration::from_secs(16));
        assert_eq!(backoff_duration(6), Duration::from_secs(32));
        assert_eq!(backoff_duration(7), Duration::from_secs(60)); // 64 → capped at 60
        assert_eq!(backoff_duration(100), Duration::from_secs(60));
    }

    #[test]
    fn test_service_state_display() {
        assert_eq!(ServiceState::Running.to_string(), "running");
        assert_eq!(ServiceState::Failed.to_string(), "failed");
    }

    /// Locate kv.wasm built by `cargo build -p zr-kv --target
    /// wasm32-wasip1 --release`. Returns None if unbuilt.
    fn kv_wasm_path() -> Option<PathBuf> {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent()?.parent()?;
        let p = workspace_root
            .join("target")
            .join("wasm32-wasip1")
            .join("release")
            .join("kv.wasm");
        p.exists().then_some(p)
    }

    /// Full daemon-side wasm-service proof: stand up the accept loop
    /// against a real wasm module, issue two separate TCP connections
    /// with different INVOKEs, verify each gets a proper OUTPUT+DONE
    /// and that state persists across connections on the same
    /// long-lived wasm instance. Closes down cleanly by dropping the
    /// handle.
    #[test]
    fn daemon_wasm_service_accept_loop_roundtrip() {
        let Some(wasm_path) = kv_wasm_path() else {
            eprintln!("skipping: build zr-kv for wasm32-wasip1 first");
            return;
        };

        let tmp = tempfile::TempDir::new().expect("tempdir");
        let zr_data = tmp.path().join(".zr").join("kv");

        // Bring up the wasm service instance — mirrors the sequence
        // `start_wasm_service` performs in production.
        let host = crate::wasm_runtime::WasmHost::shared().expect("host");
        let module = host.load_module(&wasm_path).expect("module");
        let env = vec![("ZR_DATA".to_string(), zr_data.to_string_lossy().to_string())];
        let session = host.invoke(module, env).expect("invoke");
        let crate::wasm_runtime::WasmSession {
            writer,
            reader,
            controller,
        } = session;
        drop(controller);

        // Pick an ephemeral port — production uses service.port from
        // the manifest, the test just needs a free one.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();

        let writer_arc = Arc::new(Mutex::new(writer));
        let reader_arc = Arc::new(Mutex::new(reader));
        let conn_lock = Arc::new(Mutex::new(()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let accept_thread = {
            let writer = writer_arc.clone();
            let reader = reader_arc.clone();
            let conn_lock = conn_lock.clone();
            let shutdown = shutdown.clone();
            std::thread::spawn(move || {
                wasm_service_accept_loop(
                    listener,
                    writer,
                    reader,
                    conn_lock,
                    shutdown,
                    "kv-test".to_string(),
                    "/health".to_string(),
                );
            })
        };

        // Start a capability-answering thread that drives the shared
        // wasm stdin/stdout on behalf of the TCP client. This mimics
        // what `run_protocol_session` does client-side in production.
        //
        // But wait — capabilities flow through the TCP proxy, not
        // directly on the wasm pipes. The client handles them. So
        // our TCP client role must answer capability_req frames it
        // sees on the TCP stream.

        // Connection 1: INVOKE set
        let outs_1 = tcp_invoke_with_caps(
            port,
            r#"{"type":"invoke","command":"set","args":{"key":"alpha","value":"one"}}"#,
        );
        assert_eq!(outs_1.len(), 1);
        assert_eq!(outs_1[0]["key"], "alpha");

        // Connection 2: INVOKE get — proves state persists across TCP
        // connections on the same long-lived wasm instance.
        let outs_2 = tcp_invoke_with_caps(
            port,
            r#"{"type":"invoke","command":"get","args":{"key":"alpha"}}"#,
        );
        assert_eq!(outs_2.len(), 1);
        assert_eq!(outs_2[0]["value"], "one");

        // Connection 3: HTTP health check — daemon answers directly
        // without touching wasm.
        let mut probe = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("probe");
        probe
            .write_all(b"GET /health HTTP/1.0\r\nConnection: close\r\n\r\n")
            .unwrap();
        probe.flush().unwrap();
        let mut resp = String::new();
        let _ = BufReader::new(&probe).read_to_string(&mut resp);
        assert!(resp.starts_with("HTTP/1.0 200 OK"), "health response: {resp}");

        // Shutdown: trip the flag, self-connect to unblock accept loop.
        shutdown.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(format!("127.0.0.1:{}", port));
        accept_thread.join().expect("accept thread joined");

        // Drop the session handles — wasm stdin closes, wasm exits.
        drop(writer_arc);
        drop(reader_arc);
    }

    /// TCP client that plays `run_protocol_session`'s role: sends an
    /// INVOKE, answers fs.* capability_req frames against the real
    /// filesystem, collects OUTPUT records until DONE. Closes TCP on
    /// DONE.
    fn tcp_invoke_with_caps(port: u16, invoke: &str) -> Vec<serde_json::Value> {
        let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("connect");
        let mut writer = stream.try_clone().expect("clone");
        let mut reader = BufReader::new(stream);

        writeln!(writer, "{}", invoke).expect("write invoke");
        writer.flush().expect("flush");

        let mut outputs: Vec<serde_json::Value> = Vec::new();
        loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line).expect("read line");
            if n == 0 {
                break;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let v: serde_json::Value = serde_json::from_str(trimmed).expect("parse");
            match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                "output" => outputs
                    .push(v.get("record").cloned().unwrap_or(serde_json::Value::Null)),
                "capability_req" => {
                    let id = v.get("id").and_then(|v| v.as_u64()).expect("id");
                    let op = v.get("op").and_then(|v| v.as_str()).unwrap_or("");
                    let params = v.get("params").cloned().unwrap_or(serde_json::Value::Null);
                    let reply = cap_reply(id, op, &params);
                    writeln!(writer, "{}", reply).expect("write cap_res");
                    writer.flush().expect("flush cap_res");
                }
                "done" => break,
                _ => {}
            }
        }
        outputs
    }

    fn cap_reply(id: u64, op: &str, params: &serde_json::Value) -> String {
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .map(std::path::PathBuf::from);
        let result: std::result::Result<serde_json::Value, String> = match op {
            "read_string" => {
                let p = path.unwrap();
                std::fs::read_to_string(&p)
                    .map(|c| serde_json::json!({"content": c}))
                    .map_err(|e| e.to_string())
            }
            "exists" => Ok(serde_json::json!({"exists": path.unwrap().exists()})),
            "create_dir_all" => std::fs::create_dir_all(path.unwrap())
                .map(|_| serde_json::json!({}))
                .map_err(|e| e.to_string()),
            "write" => {
                let p = path.unwrap();
                let content_b64 = params.get("content").and_then(|v| v.as_str()).unwrap();
                let bytes =
                    zacor_package::protocol::base64_decode(content_b64).expect("decode");
                std::fs::write(&p, bytes)
                    .map(|_| serde_json::json!({}))
                    .map_err(|e| e.to_string())
            }
            "rename" => {
                let from = params.get("from").and_then(|v| v.as_str()).unwrap();
                let to = params.get("to").and_then(|v| v.as_str()).unwrap();
                std::fs::rename(from, to)
                    .map(|_| serde_json::json!({}))
                    .map_err(|e| e.to_string())
            }
            _ => Err(format!("unsupported op: {op}")),
        };
        match result {
            Ok(data) => serde_json::json!({
                "type": "capability_res",
                "id": id,
                "status": "ok",
                "data": data,
            })
            .to_string(),
            Err(e) => serde_json::json!({
                "type": "capability_res",
                "id": id,
                "status": "error",
                "error": {"kind": "other", "message": e},
            })
            .to_string(),
        }
    }
}
