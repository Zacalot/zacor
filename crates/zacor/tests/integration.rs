use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn zacor_bin() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_BIN_EXE_zacor"));
    if !path.exists() {
        path = PathBuf::from("target/debug/zacor").with_extension(std::env::consts::EXE_EXTENSION);
    }
    path
}

fn zr_bin() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_BIN_EXE_zr"));
    if !path.exists() {
        path = PathBuf::from("target/debug/zr").with_extension(std::env::consts::EXE_EXTENSION);
    }
    path
}

fn temp_home() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("modules")).unwrap();
    fs::create_dir_all(dir.path().join("store")).unwrap();
    fs::create_dir_all(dir.path().join("cache")).unwrap();
    fs::create_dir_all(dir.path().join("cache").join("repos")).unwrap();
    fs::create_dir_all(dir.path().join("registries")).unwrap();
    dir
}

fn write_receipt(home: &Path, name: &str, version: &str, active: bool) {
    let receipt = serde_json::json!({
        "schema": 1,
        "current": version,
        "active": active,
        "mode": "command",
        "transport": "local",
        "config": {},
        "versions": {
            version: {
                "source": { "type": "local", "path": "/tmp/test" },
                "installed_at": "2026-03-20T10:30:00Z"
            }
        }
    });
    let path = home.join("modules").join(format!("{}.json", name));
    fs::write(&path, serde_json::to_string_pretty(&receipt).unwrap()).unwrap();
}

fn write_definition(home: &Path, name: &str, version: &str, yaml: &str) {
    let dir = home.join("store").join(name).join(version);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("package.yaml"), yaml).unwrap();
}

// ─── 12.3: Definition-only install ──────────────────────────────────

#[test]
fn test_definition_only_install() {
    let home = temp_home();
    let tmp = TempDir::new().unwrap();
    let yaml = "name: my-wrapper\nversion: \"1.0.0\"\ncommands:\n  default:\n    invoke: \"echo hello\"\n    description: test\n";
    let yaml_path = tmp.path().join("my-wrapper.yaml");
    fs::write(&yaml_path, yaml).unwrap();

    let output = Command::new(zacor_bin())
        .args(["install", &yaml_path.to_string_lossy()])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor install");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "install should succeed: {}",
        stderr
    );
    assert!(stderr.contains("installed my-wrapper"));

    // Verify receipt exists
    let receipt_path = home.path().join("modules").join("my-wrapper.json");
    assert!(receipt_path.exists(), "receipt should exist");

    // Verify definition in store
    let def_path = home
        .path()
        .join("store")
        .join("my-wrapper")
        .join("1.0.0")
        .join("package.yaml");
    assert!(def_path.exists(), "definition should be in store");

    // Verify no binary
    let store_dir = home.path().join("store").join("my-wrapper").join("1.0.0");
    let files: Vec<_> = fs::read_dir(&store_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name() != "package.yaml")
        .collect();
    assert!(
        files.is_empty(),
        "no binary should exist for definition-only package"
    );
}

// ─── 12.5: Version removal ──────────────────────────────────────────

#[test]
fn test_version_removal_switches_to_highest() {
    let home = temp_home();
    // Create receipt with multiple versions
    let receipt = serde_json::json!({
        "schema": 1,
        "current": "14.1.0",
        "active": true,
        "mode": "command",
        "transport": "local",
        "config": {},
        "versions": {
            "2.0.0": { "source": { "type": "local", "path": "/tmp/test" }, "installed_at": "2026-01-01T00:00:00Z" },
            "13.0.0": { "source": { "type": "local", "path": "/tmp/test" }, "installed_at": "2026-02-01T00:00:00Z" },
            "14.1.0": { "source": { "type": "local", "path": "/tmp/test" }, "installed_at": "2026-03-01T00:00:00Z" }
        }
    });
    fs::write(
        home.path().join("modules/tool.json"),
        serde_json::to_string_pretty(&receipt).unwrap(),
    )
    .unwrap();

    // Create store dirs
    for v in &["2.0.0", "13.0.0", "14.1.0"] {
        write_definition(
            home.path(),
            "tool",
            v,
            "name: tool\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: test\n",
        );
    }

    let output = Command::new(zacor_bin())
        .args(["remove", "tool@14.1.0"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor remove");

    assert!(
        output.status.success(),
        "remove should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify current switched to 13.0.0 (not 2.0.0 via lexicographic)
    let receipt_content = fs::read_to_string(home.path().join("modules/tool.json")).unwrap();
    let r: serde_json::Value = serde_json::from_str(&receipt_content).unwrap();
    assert_eq!(r["current"].as_str().unwrap(), "13.0.0");
}

// ─── 12.9: Package name validation ──────────────────────────────────

#[test]
fn test_package_name_validation_in_install() {
    let home = temp_home();
    let tmp = TempDir::new().unwrap();
    // Create a valid yaml but with invalid package name
    let yaml = "name: My_Tool\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: test\n";
    let yaml_path = tmp.path().join("wrapper.yaml");
    fs::write(&yaml_path, yaml).unwrap();

    let output = Command::new(zacor_bin())
        .args(["install", &yaml_path.to_string_lossy()])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor install");

    assert!(
        !output.status.success(),
        "install should fail for invalid package name"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid") || stderr.contains("must start with a lowercase"),
        "got: {}",
        stderr
    );
}

// ─── 12.13: Receipt forward-compat ──────────────────────────────────

#[test]
fn test_receipt_forward_compat() {
    let home = temp_home();
    let receipt = serde_json::json!({
        "schema": 99,
        "current": "1.0.0",
        "active": true,
        "mode": "command",
        "transport": "local",
        "config": {},
        "versions": {
            "1.0.0": { "source": { "type": "local", "path": "/tmp/test" }, "installed_at": "2026-03-20T10:30:00Z" }
        }
    });
    fs::write(
        home.path().join("modules/future-tool.json"),
        serde_json::to_string_pretty(&receipt).unwrap(),
    )
    .unwrap();

    // Dispatch should fail with upgrade guidance
    let output = Command::new(zr_bin())
        .args(["future-tool"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zr");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("newer version") || stderr.contains("upgrade"),
        "got: {}",
        stderr
    );
}

// ─── 12.15: No default command error ────────────────────────────────

#[test]
fn test_no_default_command() {
    let home = temp_home();
    write_receipt(home.path(), "my-tool", "1.0.0", true);
    write_definition(
        home.path(),
        "my-tool",
        "1.0.0",
        "name: my-tool\nversion: \"1.0.0\"\ncommands:\n  transcribe:\n    description: transcribe audio\n  translate:\n    description: translate audio\n",
    );

    let output = Command::new(zr_bin())
        .args(["my-tool"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zr");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("requires a subcommand") || stderr.contains("subcommand"),
        "should indicate a subcommand is required, got: {}",
        stderr
    );
}

// ─── 12.16: Corrupt package.yaml ────────────────────────────────────

#[test]
fn test_corrupt_package_yaml() {
    let home = temp_home();
    write_receipt(home.path(), "broken", "1.0.0", true);
    // No package.yaml in store

    let output = Command::new(zr_bin())
        .args(["broken"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zr");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("not found in store") || stderr.contains("reinstall"),
        "should suggest reinstall, got: {}",
        stderr
    );
}

// ─── Basic CLI tests ────────────────────────────────────────────────

#[test]
fn test_zr_unknown_flag() {
    let home = temp_home();
    let output = Command::new(zr_bin())
        .args(["--unknown-flag"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zr");

    assert!(!output.status.success());
}

#[test]
fn test_zr_works_with_nonexistent_home() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("nonexistent_zr_home");

    let output = Command::new(zr_bin())
        .args(["nonexistent-package"])
        .env("ZR_HOME", home.to_str().unwrap())
        .output()
        .expect("failed to run zr");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"), "got: {}", stderr);
}

// ─── Shell completions ──────────────────────────────────────────────

#[test]
fn test_completions_valid_shells() {
    let home = temp_home();
    for shell in &["bash", "zsh", "fish", "powershell"] {
        let output = Command::new(zacor_bin())
            .args(["completions", shell])
            .env("ZR_HOME", home.path().to_str().unwrap())
            .output()
            .expect("failed to run zacor completions");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "shell '{}' failed", shell);
        assert!(!stdout.is_empty(), "shell '{}' produced no output", shell);
    }
}

#[test]
fn test_completions_invalid_shell() {
    let home = temp_home();
    let output = Command::new(zacor_bin())
        .args(["completions", "invalid"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor completions");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported shell"),
        "should mention unsupported shell, got: {}",
        stderr
    );
}

// ─── Daemon integration tests ────────────────────────────────────────

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

fn daemon_request(addr: &str, req: &serde_json::Value) -> serde_json::Value {
    let mut stream = TcpStream::connect(addr).expect("failed to connect to daemon");
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .ok();
    let json = serde_json::to_string(req).unwrap();
    writeln!(stream, "{}", json).unwrap();
    stream.flush().unwrap();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("failed to read daemon response");
    serde_json::from_str(line.trim()).expect("failed to parse daemon response")
}

#[test]
#[ignore] // Uses fixed port 19100, must run in isolation: cargo test -- --ignored test_daemon_start
fn test_daemon_start_ping_status_stop() {
    let home = temp_home();

    // Start daemon in background
    let mut child = Command::new(zacor_bin())
        .args(["daemon", "start"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn daemon");

    // Wait for daemon to start
    let addr = "127.0.0.1:19100";
    let start = std::time::Instant::now();
    let mut connected = false;
    while start.elapsed() < std::time::Duration::from_secs(5) {
        if TcpStream::connect(addr).is_ok() {
            connected = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(connected, "daemon should start and accept connections");

    // Ping
    let resp = daemon_request(addr, &serde_json::json!({"request": "ping"}));
    assert_eq!(resp["ok"], true);

    // Status (no services)
    let resp = daemon_request(addr, &serde_json::json!({"request": "status"}));
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["services"].as_array().unwrap().len(), 0);

    // Shutdown
    let resp = daemon_request(addr, &serde_json::json!({"request": "shutdown"}));
    assert_eq!(resp["ok"], true);

    // Wait for daemon to exit
    let status = child.wait().expect("failed to wait for daemon");
    assert!(status.success(), "daemon should exit cleanly");
    // Allow port to be released
    std::thread::sleep(std::time::Duration::from_millis(100));
}

#[test]
fn test_daemon_stop_when_not_running() {
    let home = temp_home();
    let output = Command::new(zacor_bin())
        .args(["daemon", "stop"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor daemon stop");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("not running"), "got: {}", stdout);
}

#[test]
fn test_daemon_status_when_not_running() {
    let home = temp_home();
    let output = Command::new(zacor_bin())
        .args(["daemon", "status"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor daemon status");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("not running"), "got: {}", stdout);
}

// ─── Registry CLI integration tests ──────────────────────────────────

fn write_config(home: &Path, toml_content: &str) {
    fs::write(home.join("config.toml"), toml_content).unwrap();
}

fn create_mock_registry(home: &Path, registry_name: &str, packages: &[(&str, &str)]) {
    let reg_dir = home.join("registries").join(registry_name);
    for (pkg_name, index_toml) in packages {
        let pkg_dir = reg_dir.join("packages").join(pkg_name);
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("index.toml"), index_toml).unwrap();
    }
    // Touch sync marker so it's not stale
    fs::write(reg_dir.join(".zr-last-sync"), "").unwrap();
}

#[test]
fn test_registry_cli_add_list_remove() {
    let home = temp_home();

    // Add
    let output = Command::new(zacor_bin())
        .args([
            "registry",
            "add",
            "https://github.com/my-org/zr-packages",
            "--name",
            "company",
        ])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor registry add");
    assert!(
        output.status.success(),
        "add should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // List
    let output = Command::new(zacor_bin())
        .args(["registry", "list"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor registry list");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("company"),
        "list should show company registry, got: {}",
        stdout
    );

    // Add duplicate should fail
    let output = Command::new(zacor_bin())
        .args([
            "registry",
            "add",
            "https://example.com/other",
            "--name",
            "company",
        ])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor registry add");
    assert!(!output.status.success(), "duplicate add should fail");

    // Remove
    let output = Command::new(zacor_bin())
        .args(["registry", "remove", "company"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor registry remove");
    assert!(
        output.status.success(),
        "remove should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // List should be empty
    let output = Command::new(zacor_bin())
        .args(["registry", "list"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor registry list");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("no registries"),
        "list should be empty after remove, got: {}",
        stdout
    );

    // Remove non-existent should fail
    let output = Command::new(zacor_bin())
        .args(["registry", "remove", "nonexistent"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor registry remove");
    assert!(!output.status.success(), "remove nonexistent should fail");
}

#[test]
fn test_install_bare_name_from_mock_registry() {
    let home = temp_home();

    write_config(
        home.path(),
        "[[registries]]\nname = \"test-reg\"\nurl = \"https://example.com/test\"\n",
    );

    create_mock_registry(
        home.path(),
        "test-reg",
        &[(
            "mock-tool",
            "schema = 1\ndescription = \"A mock tool\"\n\n[[versions]]\nversion = \"1.0.0\"\nrelease = \"nonexistent/mock-tool\"\n",
        )],
    );

    // Install by bare name — should resolve from registry, then fail at GitHub download
    let output = Command::new(zacor_bin())
        .args(["install", "mock-tool"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor install");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("resolved mock-tool v1.0.0"),
        "should resolve from registry, got: {}",
        stderr
    );
}

#[test]
fn test_install_bare_name_with_version_from_mock_registry() {
    let home = temp_home();

    write_config(
        home.path(),
        "[[registries]]\nname = \"test-reg\"\nurl = \"https://example.com/test\"\n",
    );

    create_mock_registry(
        home.path(),
        "test-reg",
        &[(
            "mock-tool",
            "schema = 1\n\n[[versions]]\nversion = \"0.1.0\"\nrelease = \"nonexistent/mock-tool\"\n\n[[versions]]\nversion = \"0.2.0\"\nrelease = \"nonexistent/mock-tool\"\n",
        )],
    );

    // Install specific version
    let output = Command::new(zacor_bin())
        .args(["install", "mock-tool@0.1.0"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor install");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("resolved mock-tool v0.1.0"),
        "should resolve v0.1.0, got: {}",
        stderr
    );
}

// ─── run field: e2e dispatch ─────────────────────────────────────────

fn find_python() -> Option<String> {
    for name in &["python3", "python"] {
        if let Ok(out) = Command::new(name).arg("--version").output() {
            if out.status.success() {
                return Some(name.to_string());
            }
        }
    }
    None
}

#[test]
fn test_run_field_dispatch_e2e() {
    let python = match find_python() {
        Some(p) => p,
        None => {
            eprintln!("skipping test_run_field_dispatch_e2e: python not found");
            return;
        }
    };

    let home = temp_home();

    // Create a project directory with a run-based protocol package
    let project = TempDir::new().unwrap();
    let package_yaml = format!(
        r#"name: py-echo
version: "0.1.0"
protocol: true
run: "{python} echo.py"
commands:
  default:
    description: Echo via python
    output:
      type: text
      field: text
      schema:
        text: string
"#
    );
    fs::write(project.path().join("package.yaml"), &package_yaml).unwrap();

    // A Python script that reads one JSONL line from stdin (the INVOKE),
    // then writes OUTPUT + DONE to stdout.
    let echo_py = r#"import sys, json
line = sys.stdin.readline()
print(json.dumps({"type": "output", "record": {"text": "hello from python"}}))
print(json.dumps({"type": "done", "exit_code": 0}))
sys.stdout.flush()
"#;
    fs::write(project.path().join("echo.py"), echo_py).unwrap();

    // Install the package from the project directory
    let output = Command::new(zacor_bin())
        .args(["install", &project.path().to_string_lossy()])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor install");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "install should succeed: {}",
        stderr
    );

    // Verify echo.py was copied to the store (run package copies all files)
    let store = home.path().join("store").join("py-echo").join("0.1.0");
    assert!(store.join("echo.py").exists(), "echo.py should be in store");
    assert!(
        store.join("package.yaml").exists(),
        "package.yaml should be in store"
    );

    // Dispatch the package via zr (--json is a top-level zr flag, before the package name)
    let output = Command::new(zr_bin())
        .args(["--json", "py-echo"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zr py-echo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "dispatch should succeed.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("hello from python"),
        "output should contain greeting, got: {}",
        stdout
    );
}

#[test]
fn test_init_help_uses_feature_terminology() {
    let output = Command::new(zacor_bin())
        .args(["init", "--help"])
        .output()
        .expect("failed to run zacor init --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("Target features"));
    assert!(stdout.contains("Detects existing features if omitted"));
    assert!(!stdout.contains("Target platforms"));
}

#[test]
fn test_init_explicit_features_dispatches_positionally() {
    let home = temp_home();
    let project = TempDir::new().unwrap();

    let output = Command::new(zacor_bin())
        .args(["init", "claude-code", "gemini"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .current_dir(project.path())
        .output()
        .expect("failed to run zacor init");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "init should succeed: {stderr}");
    assert!(stderr.contains("Selected features: claude-code, gemini"));
    assert!(stderr.contains("Features synced: [claude-code, gemini]"));
    assert!(stderr.contains("Packages dispatched: 0"));
    assert!(project.path().join(".zr").is_dir());
}

#[test]
fn test_init_auto_detects_features_and_dispatches() {
    let home = temp_home();

    let project = TempDir::new().unwrap();
    fs::create_dir_all(project.path().join(".claude")).unwrap();
    fs::create_dir_all(project.path().join(".gemini")).unwrap();

    let output = Command::new(zacor_bin())
        .arg("init")
        .env("ZR_HOME", home.path().to_str().unwrap())
        .current_dir(project.path())
        .output()
        .expect("failed to run zacor init");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "init should succeed: {stderr}");
    assert!(stderr.contains("Detected features: claude-code, gemini"));
    assert!(stderr.contains("Features synced: [claude-code, gemini]"));
}

#[test]
fn test_init_rejects_invalid_feature() {
    let home = temp_home();
    let project = TempDir::new().unwrap();

    let output = Command::new(zacor_bin())
        .args(["init", "vscode"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .current_dir(project.path())
        .output()
        .expect("failed to run zacor init");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("Unknown feature 'vscode'"));
    assert!(stderr.contains("Valid features: claude-code, gemini, opencode, codex"));
}

#[test]
fn test_init_with_no_detected_features_skips_dispatch() {
    let home = temp_home();
    let project = TempDir::new().unwrap();

    let output = Command::new(zacor_bin())
        .arg("init")
        .env("ZR_HOME", home.path().to_str().unwrap())
        .current_dir(project.path())
        .output()
        .expect("failed to run zacor init");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "init should succeed: {stderr}");
    assert!(stderr.contains("Detected features:"));
    assert!(stderr.contains("No supported features detected. Nothing to sync."));
    assert!(project.path().join(".zr").is_dir());
}

#[test]
#[ignore] // Requires network and git
fn test_install_git_url() {
    let home = temp_home();

    let output = Command::new(zacor_bin())
        .args(["install", "https://github.com/zacor-packages/p-zr-core.git"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor install");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "git install should succeed: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires network and git
fn test_monorepo_git_install_reuses_cache() {
    let home = temp_home();
    let url = "https://github.com/zacor-packages/p-zr-core.git";

    let output = Command::new(zacor_bin())
        .args(["install", url])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run first install");
    assert!(
        output.status.success(),
        "first install should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let repos_dir = home.path().join("cache").join("repos");
    let repo_count = fs::read_dir(&repos_dir).map(|d| d.count()).unwrap_or(0);
    assert_eq!(repo_count, 1, "should have exactly one cached repo");
}

// ─── Wasm package dispatch ──────────────────────────────────────────

fn echo_wasm_artifact() -> Option<PathBuf> {
    let mut candidates = vec![];
    // Try workspace-relative path (most common when cargo runs tests)
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(
            cwd.join("target")
                .join("wasm32-wasip1")
                .join("release")
                .join("echo.wasm"),
        );
        if let Some(parent) = cwd.parent() {
            candidates.push(
                parent
                    .join("target")
                    .join("wasm32-wasip1")
                    .join("release")
                    .join("echo.wasm"),
            );
        }
        if let Some(grand) = cwd.parent().and_then(|p| p.parent()) {
            candidates.push(
                grand
                    .join("target")
                    .join("wasm32-wasip1")
                    .join("release")
                    .join("echo.wasm"),
            );
        }
    }
    candidates.into_iter().find(|p| p.exists())
}

#[test]
fn test_zr_dispatches_wasm_package() {
    let Some(wasm_src) = echo_wasm_artifact() else {
        eprintln!("skipping: build zr-echo for wasm32-wasip1 first");
        return;
    };

    let home = temp_home();
    // Use the crate's actual name (`echo`) so the embedded manifest's
    // `name: echo` matches the store directory.
    let store_dir = home.path().join("store").join("echo").join("0.2.0");
    fs::create_dir_all(&store_dir).unwrap();

    // Copy the wasm artifact into the store. No sidecar package.yaml —
    // dispatch reads the manifest directly from the `.wasm` via
    // wasm_manifest::load_from_store.
    fs::copy(&wasm_src, store_dir.join("echo.wasm")).unwrap();

    // Write the receipt (active, command mode).
    write_receipt(home.path(), "echo", "0.2.0", true);

    // Invoke `zr echo hello wasm dispatch`.
    let output = Command::new(zr_bin())
        .args(["echo", "hello wasm dispatch"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zr");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "dispatch should succeed. stderr: {}\nstdout: {}",
        stderr,
        stdout
    );
    assert!(
        stdout.contains("hello wasm dispatch"),
        "stdout should echo the text. stdout: {}\nstderr: {}",
        stdout,
        stderr
    );
}

fn cat_wasm_artifact() -> Option<PathBuf> {
    if let Ok(cwd) = std::env::current_dir() {
        for candidate in [
            cwd.join("target/wasm32-wasip1/release/cat.wasm"),
            cwd.parent()
                .unwrap_or(&cwd)
                .join("target/wasm32-wasip1/release/cat.wasm"),
            cwd.parent()
                .and_then(|p| p.parent())
                .unwrap_or(&cwd)
                .join("target/wasm32-wasip1/release/cat.wasm"),
        ] {
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

#[test]
fn test_zacor_install_wasm_cat_reads_real_file() {
    let Some(wasm_src) = cat_wasm_artifact() else {
        eprintln!("skipping: build zr-cat for wasm32-wasip1 first");
        return;
    };

    let home = temp_home();

    let install_out = Command::new(zacor_bin())
        .args(["install", &wasm_src.to_string_lossy()])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor install");
    assert!(
        install_out.status.success(),
        "install should succeed. stderr: {}",
        String::from_utf8_lossy(&install_out.stderr)
    );

    // Write a temp file and ask `zr cat` to read it — this exercises
    // the fs.read capability round-trip through the real CLI.
    let tmp_file = TempDir::new().unwrap();
    let file_path = tmp_file.path().join("hello.txt");
    fs::write(&file_path, "line one\nline two\nline three\n").unwrap();

    let dispatch_out = Command::new(zr_bin())
        .args(["cat", &file_path.to_string_lossy()])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zr cat");

    let stdout = String::from_utf8_lossy(&dispatch_out.stdout);
    let stderr = String::from_utf8_lossy(&dispatch_out.stderr);
    assert!(
        dispatch_out.status.success(),
        "dispatch should succeed. stderr: {}\nstdout: {}",
        stderr,
        stdout
    );
    assert!(
        stdout.contains("line one")
            && stdout.contains("line two")
            && stdout.contains("line three"),
        "cat output should include all lines. stdout: {}",
        stdout
    );
}

#[test]
fn test_zacor_install_wasm_then_dispatch() {
    let Some(wasm_src) = echo_wasm_artifact() else {
        eprintln!("skipping: build zr-echo for wasm32-wasip1 first");
        return;
    };

    let home = temp_home();

    // `zacor install path/to/echo.wasm` — the bare wasm file source.
    let install_out = Command::new(zacor_bin())
        .args(["install", &wasm_src.to_string_lossy()])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zacor install");

    let install_stderr = String::from_utf8_lossy(&install_out.stderr);
    assert!(
        install_out.status.success(),
        "install should succeed. stderr: {}",
        install_stderr
    );
    assert!(
        install_stderr.contains("installed echo"),
        "expected 'installed echo' in install output, got: {}",
        install_stderr
    );

    // Store should contain echo.wasm, no package.yaml.
    let store_dir = home.path().join("store").join("echo").join("0.2.0");
    assert!(store_dir.join("echo.wasm").exists());
    assert!(
        !store_dir.join("package.yaml").exists(),
        "wasm install should NOT write a sidecar yaml"
    );

    // Receipt should exist and be active.
    let receipt_path = home.path().join("modules").join("echo.json");
    assert!(receipt_path.exists());

    // `zr echo "installed from wasm"` — dispatch through the fully-installed package.
    let dispatch_out = Command::new(zr_bin())
        .args(["echo", "installed from wasm"])
        .env("ZR_HOME", home.path().to_str().unwrap())
        .output()
        .expect("failed to run zr");

    let dispatch_stdout = String::from_utf8_lossy(&dispatch_out.stdout);
    let dispatch_stderr = String::from_utf8_lossy(&dispatch_out.stderr);
    assert!(
        dispatch_out.status.success(),
        "dispatch should succeed. stderr: {}\nstdout: {}",
        dispatch_stderr,
        dispatch_stdout
    );
    assert!(
        dispatch_stdout.contains("installed from wasm"),
        "stdout should echo text. stdout: {}\nstderr: {}",
        dispatch_stdout,
        dispatch_stderr
    );
}
