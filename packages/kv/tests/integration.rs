use std::io::Write;
use std::process::{Command, Stdio};
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

fn run_kv(command: &str, args: serde_json::Value, data_dir: &str) -> (String, i32) {
    let invoke = serde_json::json!({
        "type": "invoke",
        "command": command,
        "args": args
    });

    let mut child = Command::new(cargo_bin())
        .env("ZR_DATA", data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start kv");

    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{}", invoke).unwrap();
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let code = output.status.code().unwrap_or(1);
    (stdout, code)
}

fn parse_output_records(stdout: &str) -> Vec<serde_json::Value> {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|msg| msg.get("type").and_then(|t| t.as_str()) == Some("output"))
        .filter_map(|msg| msg.get("record").cloned())
        .collect()
}

#[test]
fn kv_set_get_list_delete_lifecycle() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().to_string_lossy().to_string();

    // Set
    let (stdout, code) = run_kv(
        "set",
        serde_json::json!({"key": "greeting", "value": "hello world"}),
        &home,
    );
    assert_eq!(code, 0, "set failed: {}", stdout);
    let records = parse_output_records(&stdout);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["key"], "greeting");
    assert_eq!(records[0]["value"], "hello world");

    // Get
    let (stdout, code) = run_kv(
        "get",
        serde_json::json!({"key": "greeting"}),
        &home,
    );
    assert_eq!(code, 0, "get failed: {}", stdout);
    let records = parse_output_records(&stdout);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["value"], "hello world");

    // List
    let (stdout, code) = run_kv(
        "list",
        serde_json::json!({}),
        &home,
    );
    assert_eq!(code, 0, "list failed: {}", stdout);
    let records = parse_output_records(&stdout);
    assert_eq!(records.len(), 1);

    // Delete
    let (stdout, code) = run_kv(
        "delete",
        serde_json::json!({"key": "greeting"}),
        &home,
    );
    assert_eq!(code, 0, "delete failed: {}", stdout);

    // Get deleted key should fail
    let (_stdout, code) = run_kv(
        "get",
        serde_json::json!({"key": "greeting"}),
        &home,
    );
    assert_ne!(code, 0, "get of deleted key should fail");
}
