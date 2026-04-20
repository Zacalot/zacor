use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;

fn cargo_bin() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("kv");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
}

#[test]
fn kv_service_mode_set_and_get() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().to_string_lossy().to_string();
    let port = 19200; // Use a high port to avoid conflicts

    let mut child = Command::new(cargo_bin())
        .arg(format!("--listen=:{}", port))
        .env("ZR_DATA", &home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start kv service");

    // Wait for service to start
    std::thread::sleep(Duration::from_millis(500));

    // Send a SET invoke over TCP
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;

        let invoke = serde_json::json!({
            "type": "invoke",
            "command": "set",
            "args": { "key": "test_key", "value": "test_value" }
        });
        writeln!(stream, "{}", invoke)?;

        let reader = BufReader::new(&stream);
        let mut got_output = false;
        let mut got_done = false;
        for line in reader.lines() {
            let line = line?;
            if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) {
                match msg.get("type").and_then(|t| t.as_str()) {
                    Some("output") => {
                        let record = msg.get("record").unwrap();
                        assert_eq!(record["key"], "test_key");
                        assert_eq!(record["value"], "test_value");
                        got_output = true;
                    }
                    Some("done") => {
                        assert_eq!(msg["exit_code"], 0);
                        got_done = true;
                        break;
                    }
                    _ => {}
                }
            }
        }
        assert!(got_output, "Expected OUTPUT message");
        assert!(got_done, "Expected DONE message");
        Ok(())
    })();

    let _ = child.kill();
    let _ = child.wait();

    result.expect("service mode test failed");
}
