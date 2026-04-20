use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::NamedTempFile;

fn cargo_bin() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("hash");
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
fn hash_sha256_known_value() {
    // SHA-256 of "hello\n" is known
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "hello").unwrap();
    tmp.flush().unwrap();

    let file_path = tmp.path().to_string_lossy().to_string();
    let invoke = serde_json::json!({
        "type": "invoke",
        "command": "default",
        "args": { "file": file_path, "algorithm": "sha256" }
    });

    let mut child = Command::new(cargo_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start hash");

    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{}", invoke).unwrap();
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(output.status.code(), Some(0), "hash failed: {}", stdout);

    let records = parse_output_records(&stdout);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["algorithm"], "sha256");
    // SHA-256 of "hello" (no newline)
    assert_eq!(
        records[0]["hash"],
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
}

#[test]
fn hash_algorithm_choice_md5() {
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "test").unwrap();
    tmp.flush().unwrap();

    let file_path = tmp.path().to_string_lossy().to_string();
    let invoke = serde_json::json!({
        "type": "invoke",
        "command": "default",
        "args": { "file": file_path, "algorithm": "md5" }
    });

    let mut child = Command::new(cargo_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start hash");

    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{}", invoke).unwrap();
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(output.status.code(), Some(0), "hash md5 failed: {}", stdout);

    let records = parse_output_records(&stdout);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["algorithm"], "md5");
    // MD5 of "test"
    assert_eq!(records[0]["hash"], "098f6bcd4621d373cade4e832627b4f6");
}
