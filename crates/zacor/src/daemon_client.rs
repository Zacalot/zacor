use crate::error::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;

const DAEMON_PORT: u16 = 19100;

#[derive(Debug, Serialize)]
struct DaemonRequest {
    request: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
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
    let exe = std::env::current_exe().context("failed to determine current executable path")?;
    // The daemon is started via `zacor daemon start`
    // We use the same binary (zacor) with daemon start subcommand
    let zacor_bin = if exe
        .file_stem()
        .is_some_and(|s| s.to_string_lossy().starts_with("zr"))
    {
        // We're running as `zr`, find the `zacor` binary alongside it
        let parent = exe.parent().unwrap_or(Path::new("."));
        let zacor_name = if cfg!(windows) { "zacor.exe" } else { "zacor" };
        parent.join(zacor_name)
    } else {
        exe
    };

    std::process::Command::new(&zacor_bin)
        .args(["daemon", "start"])
        .env("ZR_HOME", home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("failed to spawn daemon: {}", zacor_bin.display()))?;

    Ok(())
}

/// Request the daemon to start a service.
pub fn start_service(stream: &TcpStream, name: &str) -> Result<DaemonResponse> {
    send_request(
        stream,
        &DaemonRequest {
            request: "start-service".into(),
            name: Some(name.into()),
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
    VersionMismatch(String),
    /// Daemon running but refused for another reason (package missing, etc.).
    Refused(String),
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

    let msg = ack["error"].as_str().unwrap_or("unknown error").to_string();
    if msg.contains("version mismatch") {
        Ok(DispatchAttempt::VersionMismatch(msg))
    } else {
        Ok(DispatchAttempt::Refused(msg))
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
        DispatchAttempt::Refused(msg) => bail!("daemon dispatch refused: {}", msg),
        DispatchAttempt::VersionMismatch(msg) => {
            eprintln!("zacor: {msg}; respawning daemon");
            start_daemon(home).context("respawning daemon after version mismatch")?;
            let stream = wait_for_daemon(std::time::Duration::from_secs(5))
                .context("waiting for respawned daemon")?;
            match attempt_dispatch(stream, pkg_name, version, env)? {
                DispatchAttempt::Ok(s) => Ok(Some(s)),
                DispatchAttempt::VersionMismatch(m) | DispatchAttempt::Refused(m) => {
                    bail!("daemon respawn still refused dispatch: {}", m);
                }
            }
        }
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
