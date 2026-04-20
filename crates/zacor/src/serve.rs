//! HTTP server for remote package execution (`zacor serve`).
//!
//! - `POST /invoke/{command}` — execute a command (returns session or result)
//! - `GET /session/{id}/events` — SSE stream for session events
//! - `POST /session/{id}/respond` — capability callback response
//! - `GET /packages` — list available packages

use crate::error::*;
use crate::package_definition::PackageDefinition;
use crate::paths;
use crate::receipt;
use axum::Json;
use axum::extract::{Path as AxumPath, State};
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc};
use zacor_package::protocol::{self as proto, Message};

// ─── Stream adapters (avoids external dep) ───────────────────────────

mod stream_util {
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::sync::mpsc;

    pub struct ReceiverStream<T> {
        rx: mpsc::Receiver<T>,
    }

    impl<T> ReceiverStream<T> {
        pub fn new(rx: mpsc::Receiver<T>) -> Self {
            Self { rx }
        }
    }

    impl<T> futures_core::Stream for ReceiverStream<T> {
        type Item = T;
        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
            self.rx.poll_recv(cx)
        }
    }

    pub struct Map<S, F> {
        pub stream: S,
        pub f: F,
    }

    impl<S, F, T, U> futures_core::Stream for Map<S, F>
    where
        S: futures_core::Stream<Item = T> + Unpin,
        F: FnMut(T) -> U + Unpin,
    {
        type Item = U;
        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<U>> {
            match Pin::new(&mut self.stream).poll_next(cx) {
                Poll::Ready(Some(item)) => Poll::Ready(Some((self.f)(item))),
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            }
        }
    }
}

// ─── Server State ────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    home: PathBuf,
    sessions: Arc<std::sync::Mutex<HashMap<String, SessionHandle>>>,
}

struct SessionHandle {
    /// Module stdin writer
    module_stdin: Arc<Mutex<BufWriter<std::process::ChildStdin>>>,
    /// Event receiver (taken once by the SSE handler)
    event_rx: Option<mpsc::Receiver<SsePayload>>,
    /// When this session was created
    created_at: std::time::Instant,
}

enum SsePayload {
    Message(Message),
    Close,
}

// ─── Entry Point ─────────────────────────────────────────────────────

pub fn run(home: &Path, bind: &str, port: u16) -> Result<()> {
    if tokio::runtime::Handle::try_current().is_ok() {
        bail!("zacor serve cannot be started from within an async context");
    }
    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(run_async(home, bind, port))
}

async fn run_async(home: &Path, bind: &str, port: u16) -> Result<()> {
    let state = AppState {
        home: home.to_path_buf(),
        sessions: Arc::new(std::sync::Mutex::new(HashMap::new())),
    };

    // Clone sessions handle for cleanup task before moving state into router
    let cleanup_sessions = state.sessions.clone();

    let app = axum::Router::new()
        .route("/packages", get(handle_packages))
        .route("/invoke/{command}", post(handle_invoke))
        .route("/session/{id}/events", get(handle_session_events))
        .route("/session/{id}/respond", post(handle_session_respond))
        .with_state(state);

    let addr = format!("{}:{}", bind, port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind to {}", addr))?;

    eprintln!("zacor serve listening on {}", addr);

    // Background task: clean up stale sessions (5-minute timeout)
    tokio::spawn(async move {
        const SESSION_TIMEOUT: Duration = Duration::from_secs(300);
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let stale: Vec<String> = {
                let sessions = cleanup_sessions.lock().unwrap();
                sessions
                    .iter()
                    .filter(|(_, s)| s.created_at.elapsed() > SESSION_TIMEOUT)
                    .map(|(id, _)| id.clone())
                    .collect()
            };
            if !stale.is_empty() {
                let mut sessions = cleanup_sessions.lock().unwrap();
                for id in &stale {
                    sessions.remove(id);
                    eprintln!("session {} timed out and removed", id);
                }
            }
        }
    });

    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;

    eprintln!("server stopped");
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
    eprintln!("\nshutting down...");
}

// ─── GET /packages ───────────────────────────────────────────────────

async fn handle_packages(State(state): State<AppState>) -> impl IntoResponse {
    let packages = match list_packages(&state.home) {
        Ok(p) => p,
        Err(e) => return Json(json!({"error": e.to_string()})),
    };
    Json(json!(packages))
}

fn list_packages(home: &Path) -> Result<Vec<serde_json::Value>> {
    let receipts = receipt::list_all(home)?;
    let mut packages = Vec::new();

    for (name, r) in receipts {
        if !r.active {
            continue;
        }
        let def = match crate::wasm_manifest::load_from_store(home, &name, &r.current) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let commands: Vec<String> = def.commands.keys().cloned().collect();
        packages.push(json!({
            "name": name,
            "version": r.current,
            "description": def.description,
            "protocol": def.protocol,
            "commands": commands,
        }));
    }

    Ok(packages)
}

// ─── POST /invoke/{command} ──────────────────────────────────────────

#[derive(serde::Deserialize)]
struct InvokeBody {
    #[serde(default)]
    args: BTreeMap<String, String>,
    #[serde(default)]
    uploads: BTreeMap<String, String>,
}

async fn handle_invoke(
    AxumPath(command): AxumPath<String>,
    State(state): State<AppState>,
    Json(body): Json<InvokeBody>,
) -> axum::response::Response {
    let (pkg_name, cmd_path) = match command.split_once('/') {
        Some((pkg, cmd)) => (pkg.to_string(), cmd.to_string()),
        None => (command.clone(), "default".to_string()),
    };

    let home = &state.home;
    let receipt = match receipt::read(home, &pkg_name) {
        Ok(Some(r)) if r.active => r,
        _ => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(json!({"error": format!("package '{}' not found or not active", pkg_name)})),
            )
                .into_response();
        }
    };

    let def = match crate::wasm_manifest::load_from_store(home, &pkg_name, &receipt.current) {
        Ok(d) => d,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("failed to parse definition: {}", e)})),
            )
                .into_response();
        }
    };

    if !def.commands.contains_key(&cmd_path) {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(json!({"error": format!("command '{}' not found", cmd_path)})),
        )
            .into_response();
    }

    if def.protocol {
        let cmd_def = &def.commands[&cmd_path];
        match start_session(home, &state, &def, &receipt, &cmd_path, cmd_def, &body) {
            Ok(session_id) => (
                axum::http::StatusCode::ACCEPTED,
                Json(json!({"session": session_id})),
            )
                .into_response(),
            Err(e) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response(),
        }
    } else {
        match execute_definition_package(&def, &cmd_path, &body) {
            Ok(output) => Json(json!({"output": output, "exit_code": 0})).into_response(),
            Err(e) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string(), "exit_code": 1})),
            )
                .into_response(),
        }
    }
}

fn execute_definition_package(
    def: &PackageDefinition,
    cmd_path: &str,
    body: &InvokeBody,
) -> Result<String> {
    let cmd_def = def
        .commands
        .get(cmd_path)
        .ok_or_else(|| anyhow!("command '{}' not found", cmd_path))?;
    let invoke_template = cmd_def
        .invoke
        .as_ref()
        .ok_or_else(|| anyhow!("no invoke template for command '{}'", cmd_path))?;

    let temp_dir = tempfile::TempDir::new()?;
    let mut placeholders: BTreeMap<String, String> = body
        .args
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    for (name, content_b64) in &body.uploads {
        let decoded = proto::base64_decode(content_b64)?;
        let temp_path = temp_dir.path().join(name);
        std::fs::write(&temp_path, decoded)?;
        placeholders.insert(name.clone(), temp_path.to_string_lossy().into_owned());
    }

    let env_vars = BTreeMap::new();
    let status = crate::execute::exec_invoke(invoke_template, &env_vars, &placeholders)?;
    Ok(format!("exit code: {}", status))
}

// ─── Session Management ──────────────────────────────────────────────

fn start_session(
    home: &Path,
    state: &AppState,
    def: &PackageDefinition,
    receipt: &receipt::Receipt,
    cmd_path: &str,
    cmd_def: &crate::package_definition::CommandDefinition,
    body: &InvokeBody,
) -> Result<String> {
    let binary_name = def
        .binary
        .as_ref()
        .ok_or_else(|| anyhow!("protocol package requires a binary"))?;
    let bin_path = paths::store_binary_path(home, &def.name, &receipt.current, binary_name);
    if !bin_path.exists() {
        bail!("binary not found for '{}' v{}", def.name, receipt.current);
    }

    let mut child = Command::new(&bin_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn '{}'", def.name))?;

    let child_stdin = child.stdin.take().unwrap();
    let child_stdout = child.stdout.take().unwrap();

    let has_input = cmd_def.input.is_some();
    let invoke_msg = Message::Invoke(proto::Invoke::from_str_args(
        cmd_path, &body.args, has_input,
    ));

    let stdin_writer = Arc::new(Mutex::new(BufWriter::new(child_stdin)));
    {
        let mut w = stdin_writer.blocking_lock();
        let json_str = serde_json::to_string(&invoke_msg)?;
        writeln!(w, "{}", json_str)?;
        w.flush()?;
    }

    let session_id = format!("{:016x}", rand_id());
    let (event_tx, event_rx) = mpsc::channel::<SsePayload>(256);

    let handle = SessionHandle {
        module_stdin: stdin_writer,
        event_rx: Some(event_rx),
        created_at: std::time::Instant::now(),
    };

    state
        .sessions
        .lock()
        .unwrap()
        .insert(session_id.clone(), handle);

    let sessions = state.sessions.clone();
    let sid = session_id.clone();
    std::thread::Builder::new()
        .name("protocol-reader".into())
        .spawn(move || {
            read_protocol_loop(child_stdout, event_tx);
            let _ = child.wait();
            sessions.lock().unwrap().remove(&sid);
        })
        .context("failed to spawn protocol reader")?;

    Ok(session_id)
}

fn read_protocol_loop(stdout: std::process::ChildStdout, event_tx: mpsc::Sender<SsePayload>) {
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        let line = match line {
            Ok(l) if !l.is_empty() => l,
            Ok(_) => continue,
            Err(_) => break,
        };

        let msg: Message = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let is_done = matches!(&msg, Message::Done(_));
        if event_tx.blocking_send(SsePayload::Message(msg)).is_err() {
            break;
        }
        if is_done {
            let _ = event_tx.blocking_send(SsePayload::Close);
            break;
        }
    }
}

// ─── GET /session/{id}/events ────────────────────────────────────────

async fn handle_session_events(
    AxumPath(id): AxumPath<String>,
    State(state): State<AppState>,
) -> axum::response::Response {
    let event_rx = {
        let mut sessions = state.sessions.lock().unwrap();
        match sessions.get_mut(&id) {
            Some(session) => session.event_rx.take(),
            None => None,
        }
    };

    let event_rx = match event_rx {
        Some(rx) => rx,
        None => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(json!({"error": "session not found or already streaming"})),
            )
                .into_response();
        }
    };

    let raw_stream = stream_util::ReceiverStream::new(event_rx);
    let mapped = stream_util::Map {
        stream: raw_stream,
        f: |payload: SsePayload| -> std::result::Result<Event, std::convert::Infallible> {
            match payload {
                SsePayload::Message(msg) => {
                    let json_str = serde_json::to_string(&msg).unwrap_or_default();
                    Ok(Event::default().data(json_str))
                }
                SsePayload::Close => Ok(Event::default().event("close").data("")),
            }
        },
    };

    Sse::new(mapped)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("ping"),
        )
        .into_response()
}

// ─── POST /session/{id}/respond ──────────────────────────────────────

#[derive(serde::Deserialize)]
struct RespondBody {
    id: u64,
    #[serde(default)]
    data: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<proto::CapabilityError>,
}

async fn handle_session_respond(
    AxumPath(session_id): AxumPath<String>,
    State(state): State<AppState>,
    Json(body): Json<RespondBody>,
) -> axum::response::Response {
    let module_stdin = {
        let sessions = state.sessions.lock().unwrap();
        match sessions.get(&session_id) {
            Some(session) => session.module_stdin.clone(),
            None => {
                return (
                    axum::http::StatusCode::NOT_FOUND,
                    Json(json!({"error": "session not found"})),
                )
                    .into_response();
            }
        }
    };

    let result = if let Some(error) = body.error {
        proto::CapabilityResult::Error { error }
    } else {
        proto::CapabilityResult::Ok {
            data: body.data.unwrap_or(json!({})),
        }
    };

    let res = Message::CapabilityRes(proto::CapabilityRes {
        id: body.id,
        result,
    });

    let json_str = match serde_json::to_string(&res) {
        Ok(j) => j,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response();
        }
    };

    let mut w = module_stdin.lock().await;
    if writeln!(w, "{}", json_str).is_err() || w.flush().is_err() {
        return (
            axum::http::StatusCode::GONE,
            Json(json!({"error": "module process ended"})),
        )
            .into_response();
    }

    (axum::http::StatusCode::OK, Json(json!({"ok": true}))).into_response()
}

// ─── Helpers ─────────────────────────────────────────────────────────

fn rand_id() -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut hasher);
    std::thread::current().id().hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util;

    #[test]
    fn test_list_packages_empty() {
        let home = test_util::temp_home("serve");
        let packages = list_packages(home.path()).unwrap();
        assert!(packages.is_empty());
    }
}
