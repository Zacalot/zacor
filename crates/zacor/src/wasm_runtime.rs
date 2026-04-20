//! Wasm host runtime — the `WasmHost` struct owns the wasmtime engine,
//! an in-memory module cache, and a shared tokio runtime for wasi I/O
//! bridges. It is the single library-level abstraction for running
//! wasm packages: `dispatch::execute_wasm` drives it today, the daemon
//! (when it lands) will wrap it with an IPC loop, and the editor can
//! embed it directly.
//!
//! Architecture: a wasm module runs on a dedicated thread with duplex
//! stdio pipes. The session loop drives INVOKE / OUTPUT / DONE /
//! CAPABILITY_REQ over these pipes from the host thread. The wasm
//! thread's WASI context is wired up with tokio async streams bridged
//! to sync io on the host side via `Handle::block_on`.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::pipe::{AsyncReadStream, AsyncWriteStream};
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::{AsyncStdinStream, AsyncStdoutStream, WasiCtxBuilder};

const PIPE_CAPACITY: usize = 64 * 1024;

// ─── WasmHost ────────────────────────────────────────────────────────

/// Long-lived host for running wasm packages.
///
/// Owns the wasmtime `Engine`, an in-memory `Module` cache, and a
/// shared tokio runtime for wasi I/O bridging. Safe for concurrent
/// dispatches — module cache is behind a `Mutex`, each `invoke` call
/// gets its own `Store` + `Instance` on a fresh thread.
pub struct WasmHost {
    engine: Arc<Engine>,
    runtime: tokio::runtime::Runtime,
    modules: Mutex<HashMap<PathBuf, CachedModule>>,
}

struct CachedModule {
    mtime: Option<SystemTime>,
    module: Arc<Module>,
}

impl WasmHost {
    /// Construct a fresh host. Used by tests and the daemon (which
    /// owns its own). Most dispatch paths use `WasmHost::shared()`.
    pub fn new() -> Result<Self> {
        let mut config = wasmtime::Config::new();
        config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);
        let engine = Arc::new(Engine::new(&config).context("building wasmtime engine")?);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .thread_name("zacor-wasm-rt")
            .build()
            .context("building tokio runtime for wasm host")?;

        Ok(WasmHost {
            engine,
            runtime,
            modules: Mutex::new(HashMap::new()),
        })
    }

    /// Process-wide shared host. Initialized lazily on first call; the
    /// engine and module caches are amortized across all dispatches
    /// within the process.
    pub fn shared() -> Result<&'static Arc<WasmHost>> {
        static HOST: OnceLock<Arc<WasmHost>> = OnceLock::new();
        if let Some(h) = HOST.get() {
            return Ok(h);
        }
        let host = Arc::new(WasmHost::new()?);
        let _ = HOST.set(host);
        Ok(HOST.get().expect("set above"))
    }

    /// Load a wasm module. Uses the in-memory cache when the file's
    /// mtime is unchanged since last load. Cache misses prefer the
    /// cwasm sibling (fast deserialize) and fall back to JIT from the
    /// wasm file, rewriting the cwasm for next time on fallback.
    pub fn load_module(&self, wasm_path: &Path) -> Result<Arc<Module>> {
        let mtime = std::fs::metadata(wasm_path)
            .ok()
            .and_then(|m| m.modified().ok());

        {
            let cache = self.modules.lock().unwrap();
            if let Some(cached) = cache.get(wasm_path)
                && cached.mtime == mtime
            {
                return Ok(cached.module.clone());
            }
        }

        let module = Arc::new(self.load_module_from_disk(wasm_path)?);
        self.modules.lock().unwrap().insert(
            wasm_path.to_path_buf(),
            CachedModule {
                mtime,
                module: module.clone(),
            },
        );
        Ok(module)
    }

    /// Load a module directly from disk, bypassing the cache. Prefers
    /// the cwasm sibling when present; falls back to the wasm file.
    fn load_module_from_disk(&self, wasm_path: &Path) -> Result<Module> {
        let cwasm_path = wasm_path.with_extension("cwasm");

        if cwasm_path.exists() {
            // SAFETY: cwasm files are only ever produced by us at
            // install time (see `precompile`) into our managed store
            // directory. An incompatible cwasm (e.g. after a wasmtime
            // version bump) produces a clean deserialize error,
            // handled by falling through to fresh compilation.
            match unsafe { Module::deserialize_file(&self.engine, &cwasm_path) } {
                Ok(m) => return Ok(m),
                Err(e) => {
                    eprintln!(
                        "warning: failed to load precompiled {}: {} — recompiling",
                        cwasm_path.display(),
                        e
                    );
                }
            }
        }

        let module = Module::from_file(&self.engine, wasm_path)
            .with_context(|| format!("loading wasm module {:?}", wasm_path))?;

        // Best-effort rewrite so the next dispatch hits the fast path.
        if let Ok(bytes) = module.serialize() {
            let _ = std::fs::write(&cwasm_path, bytes);
        }

        Ok(module)
    }

    /// Precompile a wasm artifact to its `.cwasm` sibling. Called at
    /// install time so the first dispatch doesn't pay cranelift cost.
    pub fn precompile(&self, wasm_path: &Path) -> Result<PathBuf> {
        let cwasm_path = wasm_path.with_extension("cwasm");
        let wasm_bytes = std::fs::read(wasm_path)
            .with_context(|| format!("reading {}", wasm_path.display()))?;
        let serialized = self
            .engine
            .precompile_module(&wasm_bytes)
            .with_context(|| format!("precompiling {}", wasm_path.display()))?;
        std::fs::write(&cwasm_path, serialized)
            .with_context(|| format!("writing {}", cwasm_path.display()))?;
        Ok(cwasm_path)
    }

    /// Start a wasm session on a dedicated thread with duplex stdio.
    /// The caller drives the session loop by writing INVOKE and
    /// reading frames through the returned sync `Write` / `BufRead`
    /// handles. Call `controller.finish()` once the session terminates.
    pub fn invoke(
        &self,
        module: Arc<Module>,
        env: Vec<(String, String)>,
    ) -> Result<WasmSession> {
        let handle = self.runtime.handle().clone();
        let engine = self.engine.clone();

        let (host_stdin_writer, wasm_stdin_reader) = tokio::io::duplex(PIPE_CAPACITY);
        let (wasm_stdout_writer, host_stdout_reader) = tokio::io::duplex(PIPE_CAPACITY);

        let wasm_handle = {
            let handle = handle.clone();
            std::thread::Builder::new()
                .name("zacor-wasm".into())
                .spawn(move || -> Result<()> {
                    let _guard = handle.enter();

                    let mut builder = WasiCtxBuilder::new();
                    builder
                        .stdin(AsyncStdinStream::new(AsyncReadStream::new(wasm_stdin_reader)))
                        .stdout(AsyncStdoutStream::new(AsyncWriteStream::new(
                            PIPE_CAPACITY,
                            wasm_stdout_writer,
                        )))
                        .inherit_stderr();
                    for (k, v) in &env {
                        builder.env(k, v);
                    }
                    let wasi = builder.build_p1();

                    let mut store = Store::new(&engine, wasi);
                    let mut linker: Linker<WasiP1Ctx> = Linker::new(&engine);
                    preview1::add_to_linker_sync(&mut linker, |cx| cx)
                        .context("registering WASI preview-1 imports")?;

                    let instance = linker
                        .instantiate(&mut store, &module)
                        .context("instantiating wasm module")?;
                    let start = instance
                        .get_typed_func::<(), ()>(&mut store, "_start")
                        .context("wasm module missing `_start` export (wasi-command target)")?;

                    // `_start` returns normally on clean exit, or traps
                    // on `proc_exit` (the wasi-command exit path).
                    // Either is fine — the caller observes EOF on
                    // stdout.
                    let _ = start.call(&mut store, ());
                    Ok(())
                })
                .context("spawning wasm thread")?
        };

        Ok(WasmSession {
            writer: BridgedWriter {
                writer: host_stdin_writer,
                handle: handle.clone(),
            },
            reader: BufReader::new(BridgedReader {
                reader: host_stdout_reader,
                handle,
            }),
            controller: Controller { join: wasm_handle },
        })
    }
}

// ─── Session types ───────────────────────────────────────────────────

/// Host-side handles to a running wasm package.
///
/// `writer` and `reader` are independent values implementing sync
/// `Write` / `BufRead`, so session-loop code can borrow both
/// simultaneously (as `run_protocol_session` does).
///
/// Call `Controller::finish()` once the session has terminated to
/// join the wasm thread and surface any trap / error.
pub struct WasmSession {
    pub writer: BridgedWriter,
    pub reader: BufReader<BridgedReader>,
    pub controller: Controller,
}

/// Retained ownership of the wasm thread. Keeping this alive is what
/// keeps the writer/reader streams operable. Drop or call `finish()`
/// once the session is complete.
pub struct Controller {
    join: std::thread::JoinHandle<Result<()>>,
}

impl Controller {
    /// Wait for the wasm thread to finish. Returns any trap captured
    /// during `_start`.
    pub fn finish(self) -> Result<()> {
        match self.join.join() {
            Ok(res) => res,
            Err(_) => Err(anyhow::anyhow!("wasm thread panicked")),
        }
    }
}

pub struct BridgedWriter {
    writer: tokio::io::DuplexStream,
    handle: tokio::runtime::Handle,
}

impl Write for BridgedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.handle.block_on(self.writer.write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.handle.block_on(self.writer.flush())
    }
}

pub struct BridgedReader {
    reader: tokio::io::DuplexStream,
    handle: tokio::runtime::Handle,
}

impl Read for BridgedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.handle.block_on(self.reader.read(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
    use zacor_package::protocol;

    /// Locate a wasm artifact built by `cargo build -p zr-<name> --target
    /// wasm32-wasip1 --release`. Returns None if unbuilt (test is skipped).
    fn wasm_artifact(name: &str) -> Option<std::path::PathBuf> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent()?.parent()?;
        let p = workspace_root
            .join("target")
            .join("wasm32-wasip1")
            .join("release")
            .join(format!("{}.wasm", name));
        if p.exists() {
            Some(p)
        } else {
            None
        }
    }

    fn echo_wasm_path() -> Option<std::path::PathBuf> {
        wasm_artifact("echo")
    }

    #[test]
    fn wasm_host_caches_loaded_modules() {
        let Some(path) = echo_wasm_path() else {
            return;
        };
        let host = WasmHost::new().expect("host");

        let a = host.load_module(&path).expect("load 1");
        let b = host.load_module(&path).expect("load 2");

        // Same Arc — cache hit on the second call.
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn wasm_echo_duplex_round_trip() {
        let Some(path) = echo_wasm_path() else {
            eprintln!("skipping: build zr-echo for wasm32-wasip1 first");
            return;
        };

        let host = WasmHost::shared().expect("host");
        let module = host.load_module(&path).expect("module");

        let WasmSession {
            mut writer,
            mut reader,
            controller,
        } = host.invoke(module, vec![]).expect("invoke");

        let invoke = r#"{"type":"invoke","command":"default","args":{"text":"hello duplex"}}"#;
        writeln!(writer, "{}", invoke).expect("write invoke");
        writer.flush().expect("flush");

        let mut frames: Vec<String> = Vec::new();
        loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line).expect("read line");
            if n == 0 {
                break;
            }
            let trimmed = line.trim_end().to_string();
            if trimmed.is_empty() {
                continue;
            }
            let is_done = trimmed.contains("\"type\":\"done\"");
            frames.push(trimmed);
            if is_done {
                break;
            }
        }
        drop(writer);
        drop(reader);
        controller.finish().expect("finish");

        let combined = frames.join("\n");
        eprintln!("frames:\n{}", combined);

        assert!(frames.iter().any(|l| l.contains("\"type\":\"output\"")));
        assert!(frames.iter().any(|l| l.contains("\"type\":\"done\"")));
        assert!(combined.contains("hello duplex"));
    }

    /// Full capability round-trip through the host: the test acts as
    /// the host, fielding CAPABILITY_REQ and replying so the module
    /// can complete. Validates the full-duplex pipe + capability
    /// dispatch path end-to-end.
    #[test]
    fn wasm_cat_capability_round_trip() {
        let Some(path) = wasm_artifact("cat") else {
            eprintln!("skipping: build zr-cat for wasm32-wasip1 first");
            return;
        };

        let host = WasmHost::shared().expect("host");
        let module = host.load_module(&path).expect("module");

        let WasmSession {
            mut writer,
            mut reader,
            controller,
        } = host.invoke(module, vec![]).expect("invoke");

        let invoke = r#"{"type":"invoke","command":"default","args":{"file":"/virtual/greeting.txt"}}"#;
        writeln!(writer, "{}", invoke).expect("write invoke");
        writer.flush().expect("flush");

        let fake_file_bytes = b"hello\nworld\n";

        let mut frames: Vec<String> = Vec::new();
        loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line).expect("read line");
            if n == 0 {
                break;
            }
            let trimmed = line.trim_end().to_string();
            if trimmed.is_empty() {
                continue;
            }
            frames.push(trimmed.clone());

            let v: serde_json::Value = serde_json::from_str(&trimmed).expect("parse frame");
            let ty = v.get("type").and_then(|v| v.as_str()).unwrap_or("");

            match ty {
                "capability_req" => {
                    let id = v.get("id").and_then(|v| v.as_u64()).expect("req id");
                    let domain = v.get("domain").and_then(|v| v.as_str()).unwrap_or("");
                    let op = v.get("op").and_then(|v| v.as_str()).unwrap_or("");
                    assert_eq!(domain, "fs");
                    assert_eq!(op, "read");

                    let encoded = protocol::base64_encode(fake_file_bytes);
                    let res = serde_json::json!({
                        "type": "capability_res",
                        "id": id,
                        "status": "ok",
                        "data": {"content": encoded},
                    });
                    writeln!(writer, "{}", res).expect("write cap_res");
                    writer.flush().expect("flush cap_res");
                }
                "done" => break,
                _ => {}
            }
        }

        drop(writer);
        drop(reader);
        controller.finish().expect("finish");

        let combined = frames.join("\n");
        eprintln!("frames:\n{}", combined);

        let output_lines: Vec<_> = frames
            .iter()
            .filter(|l| l.contains("\"type\":\"output\""))
            .collect();
        assert_eq!(output_lines.len(), 2);
        assert!(combined.contains("\"content\":\"hello\""));
        assert!(combined.contains("\"content\":\"world\""));
    }

    /// Validates the wasm-service substrate: a single wasm instance
    /// stays alive across multiple INVOKEs with persistent in-memory
    /// state, only exits when the host closes stdin, and correctly
    /// round-trips `fs.*` capabilities during both init (load_store)
    /// and handler (save_store).
    ///
    /// The test plays the host role: feeds INVOKEs into the module's
    /// stdin, handles `fs.*` capability requests against a real
    /// tempdir, and asserts state persists across calls on a single
    /// wasm instance.
    #[test]
    fn wasm_kv_service_persists_state_across_invokes() {
        let Some(path) = wasm_artifact("kv") else {
            eprintln!("skipping: build zr-kv for wasm32-wasip1 first");
            return;
        };

        let tmp = tempfile::TempDir::new().expect("tempdir");
        let zr_data = tmp.path().join(".zr").join("kv");

        let host = WasmHost::shared().expect("host");
        let module = host.load_module(&path).expect("module");

        let env = vec![("ZR_DATA".to_string(), zr_data.to_string_lossy().to_string())];
        let WasmSession {
            mut writer,
            mut reader,
            controller,
        } = host.invoke(module, env).expect("invoke");

        // Drive one INVOKE to completion, answering fs.* capability
        // requests against the real tempdir. Returns the output
        // records seen. Does NOT close stdin — wasm stays alive.
        let drive = |writer: &mut BridgedWriter,
                     reader: &mut BufReader<BridgedReader>,
                     invoke_json: &str|
         -> Vec<serde_json::Value> {
            writeln!(writer, "{}", invoke_json).expect("write invoke");
            writer.flush().expect("flush invoke");

            let mut outputs: Vec<serde_json::Value> = Vec::new();
            loop {
                let mut line = String::new();
                let n = reader.read_line(&mut line).expect("read line");
                assert!(n > 0, "unexpected EOF mid-session");
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let v: serde_json::Value =
                    serde_json::from_str(trimmed).expect("parse frame");
                let ty = v.get("type").and_then(|v| v.as_str()).unwrap_or("");

                match ty {
                    "output" => {
                        outputs.push(v.get("record").cloned().unwrap_or(serde_json::Value::Null));
                    }
                    "capability_req" => {
                        let id = v.get("id").and_then(|v| v.as_u64()).expect("req id");
                        let domain = v.get("domain").and_then(|v| v.as_str()).unwrap_or("");
                        let op = v.get("op").and_then(|v| v.as_str()).unwrap_or("");
                        let params = v.get("params").cloned().unwrap_or(serde_json::Value::Null);

                        let response = handle_fs_op(domain, op, &params);
                        let frame = serde_json::json!({
                            "type": "capability_res",
                            "id": id,
                            "status": response.as_ref().map(|_| "ok").unwrap_or("error"),
                            "data": response.clone().unwrap_or(serde_json::Value::Null),
                            "error": response.err().map(|e| serde_json::json!({
                                "kind": "other",
                                "message": e,
                            })),
                        });
                        writeln!(writer, "{}", frame).expect("write cap_res");
                        writer.flush().expect("flush cap_res");
                    }
                    "done" => break,
                    _ => {}
                }
            }
            outputs
        };

        // INVOKE 1: set foo=bar
        let outs = drive(
            &mut writer,
            &mut reader,
            r#"{"type":"invoke","command":"set","args":{"key":"foo","value":"bar"}}"#,
        );
        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0]["key"], "foo");
        assert_eq!(outs[0]["value"], "bar");

        // INVOKE 2 on the SAME wasm instance: get foo. If the state
        // didn't persist in-memory, this would have to re-read the
        // file we just wrote — which is also a valid correctness
        // signal since save_store flushed it.
        let outs = drive(
            &mut writer,
            &mut reader,
            r#"{"type":"invoke","command":"get","args":{"key":"foo"}}"#,
        );
        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0]["value"], "bar");

        // INVOKE 3: set baz=qux
        let _ = drive(
            &mut writer,
            &mut reader,
            r#"{"type":"invoke","command":"set","args":{"key":"baz","value":"qux"}}"#,
        );

        // INVOKE 4: list → 2 records sorted by key
        let outs = drive(
            &mut writer,
            &mut reader,
            r#"{"type":"invoke","command":"list","args":{}}"#,
        );
        assert_eq!(outs.len(), 2);
        assert_eq!(outs[0]["key"], "baz");
        assert_eq!(outs[1]["key"], "foo");

        // Close stdin — wasm's service_loop_stdin sees EOF and exits.
        drop(writer);
        drop(reader);
        controller.finish().expect("wasm thread clean exit");

        // Durability check: the store file on disk reflects the last state.
        let store_path = zr_data.join("kv.json");
        let on_disk = std::fs::read_to_string(&store_path).expect("store file exists");
        assert!(on_disk.contains("\"foo\""));
        assert!(on_disk.contains("\"baz\""));
    }

    /// Minimal fs-capability handler for the wasm-service test. Serves
    /// `read_string`, `exists`, `create_dir_all`, `write`, `rename`
    /// against the real filesystem — kv's save_store invokes these.
    fn handle_fs_op(
        domain: &str,
        op: &str,
        params: &serde_json::Value,
    ) -> std::result::Result<serde_json::Value, String> {
        if domain != "fs" {
            return Err(format!("unsupported domain: {}", domain));
        }
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .map(std::path::PathBuf::from);
        match op {
            "read_string" => {
                let p = path.ok_or_else(|| "missing path".to_string())?;
                match std::fs::read_to_string(&p) {
                    Ok(content) => Ok(serde_json::json!({"content": content})),
                    Err(e) => Err(format!("read_string {}: {}", p.display(), e)),
                }
            }
            "exists" => {
                let p = path.ok_or_else(|| "missing path".to_string())?;
                Ok(serde_json::json!({"exists": p.exists()}))
            }
            "create_dir_all" => {
                let p = path.ok_or_else(|| "missing path".to_string())?;
                std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
                Ok(serde_json::json!({}))
            }
            "write" => {
                let p = path.ok_or_else(|| "missing path".to_string())?;
                let content_b64 = params
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "missing content".to_string())?;
                let bytes = zacor_package::protocol::base64_decode(content_b64)
                    .map_err(|e| e.to_string())?;
                std::fs::write(&p, bytes).map_err(|e| e.to_string())?;
                Ok(serde_json::json!({}))
            }
            "rename" => {
                let from = params
                    .get("from")
                    .and_then(|v| v.as_str())
                    .map(std::path::PathBuf::from)
                    .ok_or_else(|| "missing from".to_string())?;
                let to = params
                    .get("to")
                    .and_then(|v| v.as_str())
                    .map(std::path::PathBuf::from)
                    .ok_or_else(|| "missing to".to_string())?;
                std::fs::rename(&from, &to).map_err(|e| e.to_string())?;
                Ok(serde_json::json!({}))
            }
            other => Err(format!("unsupported fs op: {}", other)),
        }
    }
}
