use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

fn cargo_bin(name: &str) -> PathBuf {
    if let Ok(path) = std::env::var(format!("CARGO_BIN_EXE_{name}")) {
        let path = PathBuf::from(path);
        if path.exists() {
            return path;
        }
    }

    let workspace_root = workspace_root();
    let mut path = workspace_root.join("target").join("debug").join(name);
    path.set_extension(std::env::consts::EXE_EXTENSION);
    if path.exists() {
        return path;
    }

    let status = Command::new("cargo")
        .args(["build", "-p", name, "--bin", name])
        .current_dir(&workspace_root)
        .status()
        .expect("failed to build binary for integration test");
    assert!(status.success(), "cargo build failed for binary {name}");
    path
}

fn workspace_root() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path
}

fn zr_bin() -> PathBuf {
    cargo_bin("zr")
}

fn zr_daemon_bin() -> PathBuf {
    cargo_bin("zr-daemon")
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

fn wait_for_daemon(timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if TcpStream::connect("127.0.0.1:19100").is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("daemon did not start within {:?}", timeout);
}

fn wait_for_daemon_exit(timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if TcpStream::connect("127.0.0.1:19100").is_err() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("daemon did not exit within {:?}", timeout);
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

fn write_kv_definition(home: &Path, version: &str, binary_name: &str) {
    let yaml = format!(
        "name: kv\nversion: \"{version}\"\nbinary: {binary_name}\ndescription: \"Key-value store\"\nprotocol: true\nproject-data: true\ncommands:\n  set:\n    description: \"Store a key-value pair\"\n    args:\n      key:\n        type: string\n        required: true\n      value:\n        type: string\n        required: true\n    output:\n      type: record\n      schema:\n        key: string\n        value: string\n  get:\n    description: \"Retrieve a value by key\"\n    args:\n      key:\n        type: string\n        required: true\n    output:\n      type: record\n      schema:\n        key: string\n        value: string\n  list:\n    description: \"List all key-value pairs\"\n    output:\n      type: table\n      schema:\n        key: string\n        value: string\n  delete:\n    description: \"Remove a key-value pair\"\n    args:\n      key:\n        type: string\n        required: true\n    output:\n      type: record\n      schema:\n        key: string\n        value: string\nexecution:\n  default: command\nservice:\n  start: \"{binary_name} --listen=:{{port}}\"\n  port: 9200\n  health: /health\n"
    );

    let dir = home.join("store").join("kv").join(version);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("package.yaml"), yaml).unwrap();
}

fn install_kv_binary(home: &Path, version: &str) {
    let zr = zr_bin();
    let kv_bin = zr
        .parent()
        .expect("zr parent")
        .join(format!("kv{}", std::env::consts::EXE_SUFFIX));
    assert!(kv_bin.exists(), "kv binary missing at {}", kv_bin.display());

    let binary_name = "kv".to_string();
    write_receipt(home, "kv", version, true);
    write_kv_definition(home, version, &binary_name);
    fs::copy(
        &kv_bin,
        home.join("store")
            .join("kv")
            .join(version)
            .join(format!("{}{}", binary_name, std::env::consts::EXE_SUFFIX)),
    )
    .unwrap();
}

fn start_daemon(home: &Path) -> Child {
    Command::new(zr_bin())
        .args(["daemon", "start"])
        .env("ZR_HOME", home)
        .env("ZR_DAEMON_BIN", zr_daemon_bin())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn daemon")
}

fn send_version_mismatch_request() {
    let mut stream = TcpStream::connect("127.0.0.1:19100").expect("connect daemon");
    writeln!(
        stream,
        "{}",
        serde_json::json!({
            "request": "dispatch",
            "pkg_name": "kv",
            "version": "0.1.0",
            "zacor_version": "999.0.0",
            "env": {}
        })
    )
    .unwrap();
    stream.flush().unwrap();

    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    assert!(line.contains("version_mismatch") || line.contains("version mismatch"), "ack: {line}");
}

fn start_http_server() -> (std::thread::JoinHandle<()>, String) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind http server");
    let addr = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
            }
            let body = b"ok";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
            stream.flush().unwrap();
        }
    });
    (handle, addr)
}

#[test]
fn daemon_version_mismatch_drains_then_client_recovers() {
    let home = temp_home();
    install_kv_binary(home.path(), "0.1.0");

    let mut daemon = start_daemon(home.path());
    wait_for_daemon(Duration::from_secs(5));

    send_version_mismatch_request();

    let output = Command::new(zr_bin())
        .args(["kv", "set", "probe", "ok"])
        .env("ZR_HOME", home.path())
        .env("ZR_DAEMON_BIN", zr_daemon_bin())
        .output()
        .expect("run zr kv get");

    assert!(
        output.status.success(),
        "zr invocation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("probe") || stdout.contains("ok"), "stdout: {stdout}");

    let _ = Command::new(zr_bin())
        .args(["daemon", "stop"])
        .env("ZR_HOME", home.path())
        .env("ZR_DAEMON_BIN", zr_daemon_bin())
        .output();
    let _ = daemon.kill();
    let _ = daemon.wait();
    wait_for_daemon_exit(Duration::from_secs(5));
}

#[test]
fn http_capability_is_forwarded_through_daemon() {
    let home = temp_home();
    let mut daemon = start_daemon(home.path());
    wait_for_daemon(Duration::from_secs(5));

    let (server_handle, url) = start_http_server();

    let mut stream = TcpStream::connect("127.0.0.1:19100").expect("connect daemon");
    writeln!(
        stream,
        "{}",
        serde_json::json!({
            "request": "capability-forward",
            "domain": "http",
            "op": "fetch",
            "params": {
                "url": url,
                "method": "GET",
                "timeout_ms": 5_000
            }
        })
    )
    .unwrap();
    stream.flush().unwrap();

    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    let response: serde_json::Value = serde_json::from_str(line.trim()).expect("parse daemon response");

    let _ = Command::new(zr_bin())
        .args(["daemon", "stop"])
        .env("ZR_HOME", home.path())
        .env("ZR_DAEMON_BIN", zr_daemon_bin())
        .output();
    let _ = daemon.kill();
    let _ = daemon.wait();
    let _ = server_handle.join();

    assert!(response["ok"].as_bool().unwrap_or(false), "response: {response}");
    let result = response
        .get("result")
        .cloned()
        .expect("capability result present");
    let capability_res: serde_json::Value = result;
    assert_eq!(capability_res["status"], "ok");
    assert_eq!(capability_res["data"]["status"], 200);
}
