use std::io::Write;
use std::process::{Command, Stdio};

fn cargo_bin() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("json");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
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
fn json_pretty_print() {
    let input_json = r#"{"a":1,"b":2}"#;
    let invoke = serde_json::json!({
        "type": "invoke",
        "command": "default",
        "args": { "indent": "2" },
        "input": true
    });
    let input_msg = serde_json::json!({
        "type": "input",
        "data": input_json
    });
    let input_end = serde_json::json!({
        "type": "input",
        "data": "",
        "eof": true
    });

    let mut child = Command::new(cargo_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start json");

    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{}", invoke).unwrap();
    writeln!(stdin, "{}", input_msg).unwrap();
    writeln!(stdin, "{}", input_end).unwrap();
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(output.status.code(), Some(0), "json failed: {}\nstderr: {}", stdout, String::from_utf8_lossy(&output.stderr));

    let records = parse_output_records(&stdout);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["valid"], true);
    // Pretty-printed output should contain newlines
    let output_str = records[0]["output"].as_str().unwrap();
    assert!(output_str.contains('\n'), "Expected pretty output: {}", output_str);
}

#[test]
fn json_compact_mode() {
    let input_json = "{\n  \"a\": 1,\n  \"b\": 2\n}";
    let invoke = serde_json::json!({
        "type": "invoke",
        "command": "default",
        "args": { "compact": "true" },
        "input": true
    });
    let input_msg = serde_json::json!({
        "type": "input",
        "data": input_json
    });
    let input_end = serde_json::json!({
        "type": "input",
        "data": "",
        "eof": true
    });

    let mut child = Command::new(cargo_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start json");

    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{}", invoke).unwrap();
    writeln!(stdin, "{}", input_msg).unwrap();
    writeln!(stdin, "{}", input_end).unwrap();
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(output.status.code(), Some(0), "json compact failed: {}", stdout);

    let records = parse_output_records(&stdout);
    assert_eq!(records.len(), 1);
    let output_str = records[0]["output"].as_str().unwrap();
    assert_eq!(output_str, r#"{"a":1,"b":2}"#);
}

#[test]
fn json_validate_valid() {
    let invoke = serde_json::json!({
        "type": "invoke",
        "command": "default",
        "args": { "validate": "true" },
        "input": true
    });
    let input_msg = serde_json::json!({
        "type": "input",
        "data": r#"{"valid": true}"#
    });
    let input_end = serde_json::json!({
        "type": "input",
        "data": "",
        "eof": true
    });

    let mut child = Command::new(cargo_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start json");

    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{}", invoke).unwrap();
    writeln!(stdin, "{}", input_msg).unwrap();
    writeln!(stdin, "{}", input_end).unwrap();
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0), "validate should succeed for valid JSON");
}

#[test]
fn json_validate_invalid() {
    let invoke = serde_json::json!({
        "type": "invoke",
        "command": "default",
        "args": { "validate": "true" },
        "input": true
    });
    let input_msg = serde_json::json!({
        "type": "input",
        "data": "not valid json {"
    });
    let input_end = serde_json::json!({
        "type": "input",
        "data": "",
        "eof": true
    });

    let mut child = Command::new(cargo_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start json");

    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{}", invoke).unwrap();
    writeln!(stdin, "{}", input_msg).unwrap();
    writeln!(stdin, "{}", input_end).unwrap();
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    assert_ne!(output.status.code(), Some(0), "validate should fail for invalid JSON");
}
