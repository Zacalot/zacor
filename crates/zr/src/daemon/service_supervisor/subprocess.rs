use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use super::super::MAX_BACKOFF;

pub(in crate::daemon) fn wait_for_health(port: u16, health_path: &str, timeout: Duration) -> bool {
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

pub(in crate::daemon) fn check_health(port: u16, health_path: &str) -> bool {
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
    status_line.contains("200")
}

pub(in crate::daemon) fn backoff_duration(failures: u32) -> Duration {
    let shift = failures.saturating_sub(1).min(63);
    let secs = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
    Duration::from_secs(secs.min(MAX_BACKOFF.as_secs()))
}
