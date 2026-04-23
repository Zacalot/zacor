use super::*;
use std::io::{BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use super::service_supervisor::ServiceState;
use super::service_supervisor::subprocess::backoff_duration;
use super::service_supervisor::wasm::wasm_service_accept_loop;

#[test]
fn test_backoff_duration() {
    assert_eq!(backoff_duration(1), Duration::from_secs(1));
    assert_eq!(backoff_duration(2), Duration::from_secs(2));
    assert_eq!(backoff_duration(3), Duration::from_secs(4));
    assert_eq!(backoff_duration(4), Duration::from_secs(8));
    assert_eq!(backoff_duration(5), Duration::from_secs(16));
    assert_eq!(backoff_duration(6), Duration::from_secs(32));
    assert_eq!(backoff_duration(7), Duration::from_secs(60));
    assert_eq!(backoff_duration(100), Duration::from_secs(60));
}

#[test]
fn test_service_state_display() {
    assert_eq!(ServiceState::Running.to_string(), "running");
    assert_eq!(ServiceState::Failed.to_string(), "failed");
}

#[test]
fn dispatch_drain_preserves_version_mismatch_reason() {
    let control = DaemonControl::new();
    control.begin_dispatch_drain(DaemonRefusal::VersionMismatch {
        daemon: "0.1.0".into(),
        client: "0.2.0".into(),
    });

    let refusal = control.dispatch_refusal(Some("0.2.1")).expect("refusal");
    assert!(matches!(
        refusal,
        DaemonRefusal::VersionMismatch { daemon, client }
        if daemon == env!("CARGO_PKG_VERSION") && client == "0.2.1"
    ));
}

fn kv_wasm_path() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent()?.parent()?;
    let p = workspace_root
        .join("target")
        .join("wasm32-wasip1")
        .join("release")
        .join("kv.wasm");
    p.exists().then_some(p)
}

#[test]
fn daemon_wasm_service_accept_loop_roundtrip() {
    let Some(wasm_path) = kv_wasm_path() else {
        eprintln!("skipping: build zr-kv for wasm32-wasip1 first");
        return;
    };

    let tmp = tempfile::TempDir::new().expect("tempdir");
    let zr_data = tmp.path().join(".zr").join("kv");

    let host = crate::wasm_runtime::WasmHost::shared().expect("host");
    let module = host.load_module(&wasm_path).expect("module");
    let env = vec![("ZR_DATA".to_string(), zr_data.to_string_lossy().to_string())];
    let session = host.invoke(module, env).expect("invoke");
    let crate::wasm_runtime::WasmSession {
        writer,
        reader,
        controller,
    } = session;
    drop(controller);

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();

    let writer_arc = Arc::new(Mutex::new(writer));
    let reader_arc = Arc::new(Mutex::new(reader));
    let conn_lock = Arc::new(Mutex::new(()));
    let shutdown = Arc::new(AtomicBool::new(false));

    let accept_thread = {
        let writer = writer_arc.clone();
        let reader = reader_arc.clone();
        let conn_lock = conn_lock.clone();
        let shutdown = shutdown.clone();
        std::thread::spawn(move || {
            wasm_service_accept_loop(
                listener,
                writer,
                reader,
                conn_lock,
                shutdown,
                "kv-test".to_string(),
                "/health".to_string(),
            );
        })
    };

    let outs_1 = tcp_invoke_with_caps(
        port,
        r#"{"type":"invoke","command":"set","args":{"key":"alpha","value":"one"}}"#,
    );
    assert_eq!(outs_1.len(), 1);
    assert_eq!(outs_1[0]["key"], "alpha");

    let outs_2 = tcp_invoke_with_caps(
        port,
        r#"{"type":"invoke","command":"get","args":{"key":"alpha"}}"#,
    );
    assert_eq!(outs_2.len(), 1);
    assert_eq!(outs_2[0]["value"], "one");

    let mut probe = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("probe");
    probe
        .write_all(b"GET /health HTTP/1.0\r\nConnection: close\r\n\r\n")
        .unwrap();
    probe.flush().unwrap();
    let mut resp = String::new();
    let _ = std::io::BufReader::new(&probe).read_to_string(&mut resp);
    assert!(resp.starts_with("HTTP/1.0 200 OK"), "health response: {resp}");

    shutdown.store(true, Ordering::Relaxed);
    let _ = TcpStream::connect(format!("127.0.0.1:{}", port));
    accept_thread.join().expect("accept thread joined");

    drop(writer_arc);
    drop(reader_arc);
}

fn tcp_invoke_with_caps(port: u16, invoke: &str) -> Vec<serde_json::Value> {
    let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("connect");
    let mut writer = stream.try_clone().expect("clone");
    let mut reader = std::io::BufReader::new(stream);

    writeln!(writer, "{}", invoke).expect("write invoke");
    writer.flush().expect("flush");

    let mut outputs: Vec<serde_json::Value> = Vec::new();
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).expect("read line");
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(trimmed).expect("parse");
        match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "output" => outputs.push(v.get("record").cloned().unwrap_or(serde_json::Value::Null)),
            "capability_req" => {
                let id = v.get("id").and_then(|v| v.as_u64()).expect("id");
                let op = v.get("op").and_then(|v| v.as_str()).unwrap_or("");
                let params = v.get("params").cloned().unwrap_or(serde_json::Value::Null);
                let reply = cap_reply(id, op, &params);
                writeln!(writer, "{}", reply).expect("write cap_res");
                writer.flush().expect("flush cap_res");
            }
            "done" => break,
            _ => {}
        }
    }
    outputs
}

fn cap_reply(id: u64, op: &str, params: &serde_json::Value) -> String {
    let path = params
        .get("path")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from);
    let result: std::result::Result<serde_json::Value, String> = match op {
        "read_string" => {
            let p = path.unwrap();
            std::fs::read_to_string(&p)
                .map(|c| serde_json::json!({"content": c}))
                .map_err(|e| e.to_string())
        }
        "exists" => Ok(serde_json::json!({"exists": path.unwrap().exists()})),
        "create_dir_all" => std::fs::create_dir_all(path.unwrap())
            .map(|_| serde_json::json!({}))
            .map_err(|e| e.to_string()),
        "write" => {
            let p = path.unwrap();
            let content_b64 = params.get("content").and_then(|v| v.as_str()).unwrap();
            let bytes = zacor_package::protocol::base64_decode(content_b64).expect("decode");
            std::fs::write(&p, bytes)
                .map(|_| serde_json::json!({}))
                .map_err(|e| e.to_string())
        }
        "rename" => {
            let from = params.get("from").and_then(|v| v.as_str()).unwrap();
            let to = params.get("to").and_then(|v| v.as_str()).unwrap();
            std::fs::rename(from, to)
                .map(|_| serde_json::json!({}))
                .map_err(|e| e.to_string())
        }
        _ => Err(format!("unsupported op: {op}")),
    };
    match result {
        Ok(data) => serde_json::json!({
            "type": "capability_res",
            "id": id,
            "status": "ok",
            "data": data,
        })
        .to_string(),
        Err(e) => serde_json::json!({
            "type": "capability_res",
            "id": id,
            "status": "error",
            "error": {"kind": "other", "message": e},
        })
        .to_string(),
    }
}

#[test]
fn warm_library_instance_reuses_state_across_invocations() {
    let Some(wasm_path) = kv_wasm_path() else {
        eprintln!("skipping: build zr-kv for wasm32-wasip1 first");
        return;
    };

    let tmp = tempfile::TempDir::new().expect("tempdir");
    let zr_data = tmp.path().join(".zr").join("kv");
    let env = HashMap::from([(
        "ZR_DATA".to_string(),
        zr_data.to_string_lossy().to_string(),
    )]);
    let store_dir = tmp.path().join("store").join("kv").join("0.1.0");
    std::fs::create_dir_all(&store_dir).unwrap();
    std::fs::copy(&wasm_path, store_dir.join("kv.wasm")).unwrap();

    let def = crate::package_definition::PackageDefinition {
        name: "kv".into(),
        version: "0.1.0".into(),
        binary: None,
        run: None,
        wasm: Some(wasm_path.file_name().unwrap().to_string_lossy().to_string()),
        description: None,
        protocol: true,
        commands: std::collections::BTreeMap::new(),
        config: std::collections::BTreeMap::new(),
        depends: crate::package_definition::DependsSection::default(),
        service: Some(crate::package_definition::ServiceSection {
            start: None,
            port: None,
            health: None,
            startup: None,
            library: true,
            idle_timeout_secs: Some(600),
            max_concurrent: Some(2),
        }),
        execution: None,
        build: None,
        project_data: true,
    };

    let pools = Arc::new(Mutex::new(HashMap::new()));
    let service = def.service.as_ref().unwrap().clone();

    let instance = module_cache::acquire_warm_library_instance(
        &pools,
        tmp.path(),
        "kv",
        "0.1.0",
        &def,
        &service,
        &env,
    )
    .expect("instance");

    let outs = drive_warm_instance(
        &instance,
        r#"{"type":"invoke","command":"set","args":{"key":"alpha","value":"one"}}"#,
    );
    assert_eq!(outs.len(), 1);
    assert_eq!(outs[0]["key"], "alpha");
    module_cache::release_warm_library_instance(&instance);

    let instance_2 = module_cache::acquire_warm_library_instance(
        &pools,
        tmp.path(),
        "kv",
        "0.1.0",
        &def,
        &service,
        &env,
    )
    .expect("instance reuse");
    assert!(Arc::ptr_eq(&instance, &instance_2));

    let outs = drive_warm_instance(
        &instance_2,
        r#"{"type":"invoke","command":"get","args":{"key":"alpha"}}"#,
    );
    assert_eq!(outs.len(), 1);
    assert_eq!(outs[0]["value"], "one");
    module_cache::release_warm_library_instance(&instance_2);

    let removed = {
        let mut pools = pools.lock().unwrap();
        pools
            .remove(&module_cache::warm_library_pool_key("kv", "0.1.0", &env))
            .unwrap()
            .instances
    };
    for instance in removed {
        module_cache::stop_warm_library_instance(instance);
    }
}

fn drive_warm_instance(
    instance: &Arc<module_cache::WarmLibraryInstance>,
    invoke_json: &str,
) -> Vec<serde_json::Value> {
    {
        let mut writer = instance.writer.lock().unwrap();
        writer.write_all(invoke_json.as_bytes()).unwrap();
        writer.write_all(b"\n").unwrap();
        writer.flush().unwrap();
    }

    let mut outputs = Vec::new();
    loop {
        let line = {
            let mut reader = instance.reader.lock().unwrap();
            let mut line = String::new();
            let n = reader.read_line(&mut line).unwrap();
            assert!(n != 0, "warm library instance ended unexpectedly");
            line
        };
        let msg: zacor_package::protocol::Message = serde_json::from_str(line.trim()).unwrap();
        match msg {
            zacor_package::protocol::Message::Output(output) => outputs.push(output.record),
            zacor_package::protocol::Message::CapabilityReq(req) => {
                let res = crate::providers::build_default_registry().dispatch(&req);
                let msg = serde_json::to_string(&zacor_package::protocol::Message::CapabilityRes(res))
                    .unwrap();
                let mut writer = instance.writer.lock().unwrap();
                writer.write_all(msg.as_bytes()).unwrap();
                writer.write_all(b"\n").unwrap();
                writer.flush().unwrap();
            }
            zacor_package::protocol::Message::Done(done) => {
                assert_eq!(done.exit_code, 0, "unexpected done: {:?}", done.error);
                break;
            }
            _ => {}
        }
    }
    outputs
}
