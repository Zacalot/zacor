use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;

fn cargo_bin() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // remove test binary name
    path.pop(); // remove deps
    path.push("watch");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
}

#[test]
fn watch_emits_create_event() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().to_string_lossy().to_string();

    let invoke = serde_json::json!({
        "type": "invoke",
        "command": "default",
        "args": { "path": dir_path }
    });

    let mut child = Command::new(cargo_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start watch");

    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{}", invoke).unwrap();
    drop(stdin); // Don't drop stdin yet — watch needs it open

    // Give the watcher time to start
    std::thread::sleep(Duration::from_millis(500));

    // Create a file to trigger an event
    let test_file = dir.path().join("test.txt");
    std::fs::write(&test_file, "hello").unwrap();

    // Give watcher time to detect
    std::thread::sleep(Duration::from_millis(500));

    // Kill the watcher process
    let _ = child.kill();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should have at least one OUTPUT message with "create" event
    let has_create = stdout.lines().any(|line| {
        if let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) {
            msg.get("type").and_then(|t| t.as_str()) == Some("output")
                && msg
                    .get("record")
                    .and_then(|r| r.get("event"))
                    .and_then(|e| e.as_str())
                    == Some("create")
        } else {
            false
        }
    });
    assert!(has_create, "Expected create event in output: {}", stdout);
}
