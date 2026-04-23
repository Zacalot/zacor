use crate::error::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use zacor_host::protocol::DaemonRefusal;

const DAEMON_PORT: u16 = 19100;

#[derive(Debug, Serialize)]
struct DaemonRequest {
    request: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pkg_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    env: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    zacor_version: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DaemonResponse {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub services: Option<Vec<ServiceStatus>>,
}

#[derive(Debug, Deserialize)]
pub struct ServiceStatus {
    pub name: String,
    pub port: u16,
    pub status: String,
}

fn daemon_addr() -> String {
    #[cfg(unix)]
    {
        // On Unix we use a Unix domain socket, but for cross-platform simplicity
        // we fall back to TCP for now (same as Windows)
        format!("127.0.0.1:{}", DAEMON_PORT)
    }
    #[cfg(windows)]
    {
        format!("127.0.0.1:{}", DAEMON_PORT)
    }
}

fn send_request(stream: &TcpStream, req: &DaemonRequest) -> Result<DaemonResponse> {
    let mut stream = stream
        .try_clone()
        .context("failed to clone daemon stream")?;
    let json = serde_json::to_string(req).context("failed to serialize daemon request")?;
    writeln!(stream, "{}", json).context("failed to write to daemon")?;
    stream.flush().context("failed to flush daemon stream")?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .context("failed to read daemon response")?;
    serde_json::from_str(line.trim()).context("failed to parse daemon response")
}

/// Connect to the daemon. Returns None if the daemon is not running.
pub fn connect() -> Option<TcpStream> {
    TcpStream::connect(daemon_addr()).ok()
}

/// Connect to the daemon, starting it if needed.
pub fn connect_or_start_daemon(home: &Path) -> Result<TcpStream> {
    if let Some(stream) = connect() {
        return Ok(stream);
    }
    start_daemon(home)?;
    wait_for_daemon(std::time::Duration::from_secs(5))
        .context("failed to start daemon")
}

/// Start the daemon process in the background.
fn start_daemon(home: &Path) -> Result<()> {
    let daemon_bin = if let Ok(path) = std::env::var("ZR_DAEMON_BIN") {
        let path = std::path::PathBuf::from(path);
        if path.exists() {
            path
        } else {
            crate::resolve_peer_binary("zr-daemon")
        }
    } else {
        crate::resolve_peer_binary("zr-daemon")
    };

    std::process::Command::new(&daemon_bin)
        .env("ZR_HOME", home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("failed to spawn daemon: {}", daemon_bin.display()))?;

    Ok(())
}

/// Request the daemon to start a service.
pub fn start_service(stream: &TcpStream, name: &str) -> Result<DaemonResponse> {
    send_request(
        stream,
        &DaemonRequest {
            request: "start-service".into(),
            name: Some(name.into()),
            pkg_name: None,
            version: None,
            command: None,
            args: None,
            env: None,
            zacor_version: None,
        },
    )
}

/// Request daemon status.
pub fn status(stream: &TcpStream) -> Result<DaemonResponse> {
    send_request(
        stream,
        &DaemonRequest {
            request: "status".into(),
            name: None,
            pkg_name: None,
            version: None,
            command: None,
            args: None,
            env: None,
            zacor_version: None,
        },
    )
}

/// Send shutdown to the daemon.
pub fn shutdown(stream: &TcpStream) -> Result<DaemonResponse> {
    send_request(
        stream,
        &DaemonRequest {
            request: "shutdown".into(),
            name: None,
            pkg_name: None,
            version: None,
            command: None,
            args: None,
            env: None,
            zacor_version: None,
        },
    )
}

/// Outcome of attempting one dispatch handshake against the daemon.
enum DispatchAttempt {
    /// Daemon accepted — the stream is ready for wasm-protocol bytes.
    Ok(TcpStream),
    /// Daemon refused because its version differs from the client's.
    /// The daemon also self-exits in this case, so a retry against a
    /// freshly-spawned daemon should succeed.
    VersionMismatch {
        refusal: DaemonRefusal,
        message: String,
    },
    /// Daemon running but refused for another reason (package missing, etc.).
    Refused {
        refusal: Option<DaemonRefusal>,
        message: String,
    },
}

fn attempt_dispatch(
    stream: TcpStream,
    pkg_name: &str,
    version: &str,
    env: &BTreeMap<String, String>,
) -> Result<DispatchAttempt> {
    let req = serde_json::json!({
        "request": "dispatch",
        "pkg_name": pkg_name,
        "version": version,
        "zacor_version": env!("CARGO_PKG_VERSION"),
        "env": env,
    });

    let mut writer = stream
        .try_clone()
        .context("cloning daemon stream for write")?;
    writeln!(writer, "{}", req).context("sending dispatch request")?;
    writer.flush().context("flushing dispatch request")?;

    let mut ack_stream = stream.try_clone().context("cloning daemon stream for ack")?;
    let ack_line = read_line_byte_by_byte(&mut ack_stream).context("reading dispatch ack")?;

    let ack: serde_json::Value =
        serde_json::from_str(ack_line.trim()).context("parsing dispatch ack")?;

    if ack["ok"].as_bool().unwrap_or(false) {
        return Ok(DispatchAttempt::Ok(stream));
    }

    let refusal = ack
        .get("refusal")
        .cloned()
        .and_then(|value| serde_json::from_value::<DaemonRefusal>(value).ok());
    let message = refusal
        .as_ref()
        .map(refusal_message)
        .or_else(|| ack["error"].as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown error".to_string());

    match refusal {
        Some(refusal @ DaemonRefusal::VersionMismatch { .. }) => {
            Ok(DispatchAttempt::VersionMismatch { refusal, message })
        }
        Some(refusal) => Ok(DispatchAttempt::Refused {
            refusal: Some(refusal),
            message,
        }),
        None if message.contains("version mismatch") => Ok(DispatchAttempt::VersionMismatch {
            refusal: DaemonRefusal::VersionMismatch {
                daemon: "unknown".into(),
                client: "unknown".into(),
            },
            message,
        }),
        None => Ok(DispatchAttempt::Refused {
            refusal: None,
            message,
        }),
    }
}

fn attempt_library_invoke(
    stream: TcpStream,
    pkg_name: &str,
    version: &str,
    command: &str,
    args: &BTreeMap<String, String>,
    env: &BTreeMap<String, String>,
) -> Result<DispatchAttempt> {
    let req = serde_json::json!({
        "request": "invoke-library",
        "pkg_name": pkg_name,
        "version": version,
        "command": command,
        "args": args,
        "zacor_version": env!("CARGO_PKG_VERSION"),
        "env": env,
    });

    let mut writer = stream.try_clone().context("cloning daemon stream for write")?;
    writeln!(writer, "{}", req).context("sending library invoke request")?;
    writer.flush().context("flushing library invoke request")?;

    let mut ack_stream = stream.try_clone().context("cloning daemon stream for ack")?;
    let ack_line = read_line_byte_by_byte(&mut ack_stream).context("reading library invoke ack")?;
    let ack: serde_json::Value = serde_json::from_str(ack_line.trim()).context("parsing library invoke ack")?;

    if ack["ok"].as_bool().unwrap_or(false) {
        return Ok(DispatchAttempt::Ok(stream));
    }

    let refusal = ack
        .get("refusal")
        .cloned()
        .and_then(|value| serde_json::from_value::<DaemonRefusal>(value).ok());
    let message = refusal
        .as_ref()
        .map(refusal_message)
        .or_else(|| ack["error"].as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown error".to_string());

    match refusal {
        Some(refusal @ DaemonRefusal::VersionMismatch { .. }) => {
            Ok(DispatchAttempt::VersionMismatch { refusal, message })
        }
        Some(refusal) => Ok(DispatchAttempt::Refused {
            refusal: Some(refusal),
            message,
        }),
        None if message.contains("version mismatch") => Ok(DispatchAttempt::VersionMismatch {
            refusal: DaemonRefusal::VersionMismatch {
                daemon: "unknown".into(),
                client: "unknown".into(),
            },
            message,
        }),
        None => Ok(DispatchAttempt::Refused {
            refusal: None,
            message,
        }),
    }
}

/// Wait for a freshly-spawned daemon to accept TCP connections. Shared by
/// `connect_or_start_daemon` and the version-mismatch respawn path.
fn wait_for_daemon(timeout: std::time::Duration) -> Result<TcpStream> {
    let start = std::time::Instant::now();
    let interval = std::time::Duration::from_millis(50);
    loop {
        if let Some(stream) = connect() {
            return Ok(stream);
        }
        if start.elapsed() > timeout {
            bail!("daemon startup timeout after {:?}", timeout);
        }
        std::thread::sleep(interval);
    }
}

fn wait_for_daemon_exit(timeout: std::time::Duration) -> Result<()> {
    let start = std::time::Instant::now();
    let interval = std::time::Duration::from_millis(50);
    loop {
        if connect().is_none() {
            return Ok(());
        }
        if start.elapsed() > timeout {
            bail!("daemon shutdown timeout after {:?}", timeout);
        }
        std::thread::sleep(interval);
    }
}

/// Try to open a dispatch session against the running daemon.
///
/// - `Ok(Some(stream))` — daemon accepted; the stream is ready to carry
///   the wasm protocol (client sends INVOKE, reads OUTPUT/DONE, etc.).
///   The caller drives it with `run_protocol_session`.
/// - `Ok(None)` — daemon is not running; caller falls back to in-process
///   dispatch. (v1 policy: don't auto-spawn for dispatch. Users start
///   the daemon with `zacor daemon start` to get the fast path.)
/// - `Err(e)` — daemon running but refused (e.g., package not
///   installed in its ZR_HOME, wasm artifact missing). Caller falls
///   back to in-process dispatch.
///
/// Special case: on a version-mismatch refusal, the daemon self-exits
/// (it's stale relative to this client). This function then auto-spawns
/// a fresh daemon and retries the dispatch once — so users don't have
/// to manually restart after a zacor upgrade.
pub fn try_open_dispatch_stream(
    home: &Path,
    pkg_name: &str,
    version: &str,
    env: &BTreeMap<String, String>,
) -> Result<Option<TcpStream>> {
    let Some(stream) = connect() else {
        return Ok(None);
    };

    match attempt_dispatch(stream, pkg_name, version, env)? {
        DispatchAttempt::Ok(s) => Ok(Some(s)),
        DispatchAttempt::Refused { refusal, message } => match refusal {
            Some(refusal) => Err(DispatchError::DaemonRefused(refusal).into()),
            None => bail!("daemon dispatch refused: {}", message),
        },
        DispatchAttempt::VersionMismatch { message, .. } => {
            eprintln!("zacor: {message}; waiting for daemon drain");
            wait_for_daemon_exit(std::time::Duration::from_secs(65))
                .context("waiting for draining daemon to exit")?;
            start_daemon(home).context("respawning daemon after version mismatch")?;
            let stream = wait_for_daemon(std::time::Duration::from_secs(5))
                .context("waiting for respawned daemon")?;
            match attempt_dispatch(stream, pkg_name, version, env)? {
                DispatchAttempt::Ok(s) => Ok(Some(s)),
                DispatchAttempt::VersionMismatch { refusal, .. } => {
                    Err(DispatchError::DaemonRefused(refusal).into())
                }
                DispatchAttempt::Refused { refusal, message } => match refusal {
                    Some(refusal) => Err(DispatchError::DaemonRefused(refusal).into()),
                    None => bail!("daemon respawn still refused dispatch: {}", message),
                },
            }
        }
    }
}

pub fn try_open_library_invoke_stream(
    home: &Path,
    pkg_name: &str,
    version: &str,
    command: &str,
    args: &BTreeMap<String, String>,
    env: &BTreeMap<String, String>,
) -> Result<Option<TcpStream>> {
    let Some(stream) = connect() else {
        return Ok(None);
    };

    match attempt_library_invoke(stream, pkg_name, version, command, args, env)? {
        DispatchAttempt::Ok(stream) => Ok(Some(stream)),
        DispatchAttempt::Refused { refusal, message } => match refusal {
            Some(refusal) => Err(DispatchError::DaemonRefused(refusal).into()),
            None => bail!("daemon library invoke refused: {}", message),
        },
        DispatchAttempt::VersionMismatch { message, .. } => {
            eprintln!("zacor: {message}; waiting for daemon drain");
            wait_for_daemon_exit(std::time::Duration::from_secs(65))
                .context("waiting for draining daemon to exit")?;
            start_daemon(home).context("respawning daemon after version mismatch")?;
            let stream = wait_for_daemon(std::time::Duration::from_secs(5))
                .context("waiting for respawned daemon")?;
            match attempt_library_invoke(stream, pkg_name, version, command, args, env)? {
                DispatchAttempt::Ok(stream) => Ok(Some(stream)),
                DispatchAttempt::VersionMismatch { refusal, .. } => {
                    Err(DispatchError::DaemonRefused(refusal).into())
                }
                DispatchAttempt::Refused { refusal, message } => match refusal {
                    Some(refusal) => Err(DispatchError::DaemonRefused(refusal).into()),
                    None => bail!("daemon respawn still refused library invoke: {}", message),
                },
            }
        }
    }
}

fn refusal_message(refusal: &DaemonRefusal) -> String {
    match refusal {
        DaemonRefusal::VersionMismatch { daemon, client } => {
            format!("daemon version mismatch: daemon={}, client={} - daemon draining", daemon, client)
        }
        DaemonRefusal::PackageNotFound { name } => format!("package not found: {}", name),
        DaemonRefusal::WasmArtifactMissing { path } => format!("wasm artifact missing: {}", path),
        DaemonRefusal::LoadFailed { reason } => reason.clone(),
        DaemonRefusal::InvalidRequest { reason } => reason.clone(),
        DaemonRefusal::Other { message } => message.clone(),
    }
}

/// Read a single line from a stream one byte at a time. Avoids the
/// BufReader trap of pre-fetching bytes past the line terminator, which
/// would be lost when the stream is passed to the caller.
fn read_line_byte_by_byte(stream: &mut TcpStream) -> Result<String> {
    let mut line = Vec::new();
    let mut buf = [0u8; 1];
    loop {
        let n = stream.read(&mut buf).context("reading from daemon")?;
        if n == 0 {
            break;
        }
        if buf[0] == b'\n' {
            break;
        }
        line.push(buf[0]);
    }
    String::from_utf8(line).context("daemon sent non-utf8 ack")
}
