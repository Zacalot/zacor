use crate::error::*;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

const DAEMON_PORT: u16 = 19100;

#[derive(Debug, Serialize)]
struct DaemonRequest {
    request: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DaemonResponse {
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
