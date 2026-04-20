use std::collections::BTreeMap;
use std::io::{self, BufRead};

#[cfg(not(target_family = "wasm"))]
use std::io::{BufReader, Write};
#[cfg(not(target_family = "wasm"))]
use std::net::TcpListener;
#[cfg(not(target_family = "wasm"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_family = "wasm"))]
use std::sync::{Arc, Mutex};

// ─── FromArgs Trait ──────────────────────────────────────────────────

/// Parse typed arg structs directly from the INVOKE args map.
///
/// Implemented by `#[derive(ZrArgs)]` and by build-generated arg structs.
pub trait FromArgs: Sized {
    fn from_args(args: &BTreeMap<String, serde_json::Value>) -> Result<Self, String>;
}

// ─── Context & Unified Protocol ──────────────────────────────────────

/// Unified context for protocol handlers. Provides access to command name,
/// args, optional input stream, and output emission.
pub struct Context {
    invoke: crate::protocol::Invoke,
    input: Option<Box<dyn BufRead>>,
}

impl Context {
    /// Returns the command name from the INVOKE message.
    pub fn command(&self) -> &str {
        &self.invoke.command
    }

    /// Returns a reference to the raw args map from the INVOKE message.
    pub fn raw_args(&self) -> &BTreeMap<String, serde_json::Value> {
        &self.invoke.args
    }

    /// Parse typed args from the INVOKE args map.
    pub fn args<T: FromArgs>(&self) -> Result<T, String> {
        T::from_args(&self.invoke.args)
    }

    /// Take ownership of the input stream. Returns `None` if no input was
    /// provided or if input was already taken.
    pub fn input(&mut self) -> Option<Box<dyn BufRead>> {
        self.input.take()
    }

    /// Return the input stream, or an empty reader if no input was provided
    /// or input was already consumed.
    pub fn input_or_empty(&mut self) -> Box<dyn BufRead> {
        self.input.take().unwrap_or_else(|| Box::new(io::empty()))
    }

    /// Emit a single output record.
    pub fn emit(&self, record: serde_json::Value) -> Result<(), String> {
        crate::runtime::send_output(record).map_err(|e| e.to_string())
    }

    /// Emit a serializable record as an OUTPUT message.
    pub fn emit_record(&self, record: &impl serde::Serialize) -> Result<(), String> {
        let value = serde_json::to_value(record).map_err(|e| e.to_string())?;
        self.emit(value)
    }

    /// Emit multiple output records.
    pub fn emit_all(
        &self,
        records: impl IntoIterator<Item = serde_json::Value>,
    ) -> Result<(), String> {
        for record in records {
            self.emit(record)?;
        }
        Ok(())
    }
}

/// Unified protocol entry point. Replaces all `protocol_*` functions.
///
/// Reads INVOKE, initializes the runtime, creates a Context, and calls
/// the handler. The handler returns an exit code via `Ok(i32)` or an
/// error string via `Err`. All output goes through `ctx.emit()`.
pub fn protocol<E>(_package_name: &str, handler: impl FnOnce(&mut Context) -> Result<i32, E>) -> i32
where
    E: std::fmt::Display,
{
    let (invoke, input_reader) = match protocol_init() {
        Ok(v) => v,
        Err(code) => return code,
    };

    let input: Option<Box<dyn BufRead>> = input_reader.map(|r| Box::new(r) as Box<dyn BufRead>);
    let mut ctx = Context { invoke, input };

    match handler(&mut ctx) {
        Ok(exit_code) => {
            let _ = crate::runtime::send_done(exit_code, None);
            exit_code
        }
        Err(e) => {
            let _ = crate::runtime::send_done(1, Some(e.to_string()));
            1
        }
    }
}

// ─── Protocol Init ───────────────────────────────────────────────────

/// Common init sequence for protocol entry points.
/// Returns the invoke and input reader, with reader started.
fn protocol_init(
) -> std::result::Result<(crate::protocol::Invoke, Option<crate::runtime::InputReader>), i32> {
    let (invoke, input_reader, reader_handle) = match crate::runtime::init_invoke() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", e);
            return Err(1);
        }
    };
    if let Err(e) = crate::runtime::start_reader(reader_handle) {
        eprintln!("{}", e);
        return Err(1);
    }
    #[cfg(not(target_family = "wasm"))]
    crate::io::set_mode(crate::io::ExecMode::ProtocolLocal);
    #[cfg(target_family = "wasm")]
    crate::io::set_mode(crate::io::ExecMode::ProtocolRemote);
    Ok((invoke, input_reader))
}

// ─── Service Loop ────────────────────────────────────────────────────
// Service mode binds a TCP listener and spawns per-connection threads,
// neither of which exists on wasm32-wasi*. Under wasm, service semantics
// move host-side: the daemon holds the socket and dispatches INVOKEs to
// long-lived wasm instances via host-mediated I/O.

/// Run a module as a persistent TCP service.
///
/// Accepts an address (e.g., ":9100"), a state initialization function,
/// and a handler function called per invocation with mutable access to
/// shared state.
///
/// The service listens on the given TCP port. Each connection receives
/// one INVOKE, the handler executes with access to persistent state,
/// OUTPUT/DONE are sent back, and the connection closes.
///
/// HTTP GET requests to any path (e.g., /health) receive a minimal
/// `{"status":"ok"}` response for health checking.
#[cfg(not(target_family = "wasm"))]
pub fn service_loop<S, Init, Handler>(addr: &str, init_state: Init, handler: Handler) -> !
where
    S: Send + 'static,
    Init: FnOnce() -> S,
    Handler: Fn(&mut S, crate::protocol::Invoke) -> Vec<serde_json::Value> + Send + Sync + 'static,
{
    let listen_addr = if addr.starts_with(':') {
        format!("0.0.0.0{}", addr)
    } else {
        addr.to_string()
    };

    let listener = TcpListener::bind(&listen_addr).unwrap_or_else(|e| {
        eprintln!("service: failed to bind {}: {}", listen_addr, e);
        std::process::exit(1);
    });

    eprintln!("service: listening on {}", listen_addr);

    let state = Arc::new(Mutex::new(init_state()));
    let handler = Arc::new(handler);
    let shutdown = Arc::new(AtomicBool::new(false));

    // Set up signal handler for graceful shutdown
    let shutdown_flag = shutdown.clone();
    let _ = setup_shutdown_signal(shutdown_flag);

    for stream in listener.incoming() {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        let stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };

        let state = state.clone();
        let handler = handler.clone();

        std::thread::Builder::new()
            .name("zr-service-conn".into())
            .spawn(move || {
                handle_service_connection(stream, &state, &handler);
            })
            .ok();
    }

    std::process::exit(0);
}

/// Handle a single service connection.
/// Detects HTTP health checks vs protocol INVOKE.
#[cfg(not(target_family = "wasm"))]
fn handle_service_connection<S, Handler>(
    stream: std::net::TcpStream,
    state: &Arc<Mutex<S>>,
    handler: &Arc<Handler>,
) where
    Handler: Fn(&mut S, crate::protocol::Invoke) -> Vec<serde_json::Value>,
{
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });

    // Peek at first bytes to detect HTTP vs protocol
    let mut first_line = String::new();
    if reader.read_line(&mut first_line).is_err() || first_line.is_empty() {
        return;
    }

    // HTTP detection: starts with "GET " or "HEAD "
    if first_line.starts_with("GET ") || first_line.starts_with("HEAD ") {
        // Drain remaining headers
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
                break;
            }
        }
        let body = r#"{"status":"ok"}"#;
        let response = format!(
            "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let mut writer = stream;
        let _ = writer.write_all(response.as_bytes());
        let _ = writer.flush();
        return;
    }

    // Protocol: first line should be INVOKE
    let msg: crate::protocol::Message = match serde_json::from_str(first_line.trim()) {
        Ok(m) => m,
        Err(_) => return,
    };

    let invoke = match msg {
        crate::protocol::Message::Invoke(inv) => inv,
        _ => return,
    };

    // Set up protocol IO for this connection
    // The module handler may use capability calls, which need the runtime.
    // For service mode, we initialize a per-connection runtime with the TCP stream.
    let writer_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };

    // Execute handler with shared state
    let records = {
        let mut state = state.lock().unwrap();
        handler(&mut state, invoke)
    };

    // Send OUTPUT messages
    let mut writer = io::BufWriter::new(writer_stream);
    for record in records {
        let msg = crate::protocol::Message::Output(crate::protocol::Output { record });
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = writeln!(writer, "{}", json);
        }
    }

    // Send DONE
    let done = crate::protocol::Message::Done(crate::protocol::Done {
        exit_code: 0,
        error: None,
    });
    if let Ok(json) = serde_json::to_string(&done) {
        let _ = writeln!(writer, "{}", json);
    }
    let _ = writer.flush();
}

/// Run a wasm module as a persistent service driven by the host.
///
/// Unlike native `service_loop`, the wasm variant has no socket — the
/// host (daemon) owns the TCP listener and feeds INVOKE messages down
/// the module's stdin pipe, reading OUTPUT / CAPABILITY_REQ / DONE from
/// stdout. This function reads INVOKE after INVOKE from stdin in a loop,
/// calls the handler for each, emits OUTPUT+DONE per invocation, and
/// exits cleanly on stdin EOF.
///
/// State is plain `&mut S` (no Mutex / Arc) because the wasm instance
/// is single-threaded and the daemon serializes connections. Capability
/// calls inside the handler work normally — they flow through the same
/// stdin/stdout as the INVOKE/OUTPUT frames, demuxed by the protocol
/// runtime.
#[cfg(target_family = "wasm")]
pub fn service_loop_stdin<S, Init, Handler>(init_state: Init, mut handler: Handler) -> !
where
    Init: FnOnce() -> S,
    Handler: FnMut(&mut S, crate::protocol::Invoke) -> Vec<serde_json::Value>,
{
    let (mut invoke, _input_reader) = match protocol_init() {
        Ok(v) => v,
        Err(code) => std::process::exit(code),
    };

    let mut state = init_state();

    loop {
        let records = handler(&mut state, invoke);
        for record in records {
            let _ = crate::runtime::send_output(record);
        }
        let _ = crate::runtime::send_done(0, None);

        match crate::runtime::read_next_invoke() {
            Ok(Some(next)) => invoke = next,
            Ok(None) => std::process::exit(0),
            Err(_) => std::process::exit(0),
        }
    }
}

/// Set up SIGTERM/ctrl-c handler for graceful shutdown.
#[cfg(not(target_family = "wasm"))]
fn setup_shutdown_signal(shutdown: Arc<AtomicBool>) -> io::Result<()> {
    #[cfg(unix)]
    {
        // On Unix, we'd use signal handling. For now, just ctrl-c.
        let _ = shutdown;
    }
    #[cfg(windows)]
    {
        let _ = shutdown;
    }
    // ctrl-c is handled by the OS killing us; the service_loop checks shutdown flag
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(target_family = "wasm"))]
    use std::net::TcpStream;

    // ─── Context Unit Tests ──────────────────────────────────────────

    fn make_context(
        command: &str,
        args: BTreeMap<String, serde_json::Value>,
        input: Option<&str>,
    ) -> Context {
        let invoke = crate::protocol::Invoke {
            version: 1,
            command: command.into(),
            args,
            input: input.is_some(),
        };
        let input: Option<Box<dyn BufRead>> =
            input.map(|s| Box::new(io::Cursor::new(s.to_owned())) as Box<dyn BufRead>);
        Context { invoke, input }
    }

    #[test]
    fn context_command_returns_invoke_command() {
        let ctx = make_context("add", BTreeMap::new(), None);
        assert_eq!(ctx.command(), "add");
    }

    #[test]
    fn context_raw_args_returns_args_map() {
        let mut args = BTreeMap::new();
        args.insert("utc".into(), serde_json::json!(true));
        args.insert("format".into(), serde_json::json!("iso"));
        let ctx = make_context("default", args, None);
        let raw = ctx.raw_args();
        assert_eq!(raw["utc"], serde_json::json!(true));
        assert_eq!(raw["format"], serde_json::json!("iso"));
    }

    #[derive(Debug, PartialEq)]
    struct TestArgs {
        name: String,
        count: Option<f64>,
        verbose: bool,
    }

    impl FromArgs for TestArgs {
        fn from_args(args: &BTreeMap<String, serde_json::Value>) -> Result<Self, String> {
            let name = args
                .get("name")
                .and_then(|v| v.as_str())
                .map(String::from)
                .ok_or_else(|| "missing required arg: name".to_string())?;
            let count = args.get("count").and_then(|v| v.as_f64());
            let verbose = args
                .get("verbose")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            Ok(TestArgs {
                name,
                count,
                verbose,
            })
        }
    }

    #[test]
    fn context_args_parses_typed_struct() {
        let mut args = BTreeMap::new();
        args.insert("name".into(), serde_json::json!("hello"));
        args.insert("count".into(), serde_json::json!(42));
        args.insert("verbose".into(), serde_json::json!(true));
        let ctx = make_context("default", args, None);
        let parsed = ctx.args::<TestArgs>().unwrap();
        assert_eq!(parsed.name, "hello");
        assert_eq!(parsed.count, Some(42.0));
        assert!(parsed.verbose);
    }

    #[test]
    fn context_args_error_on_missing_required() {
        let ctx = make_context("default", BTreeMap::new(), None);
        let result = ctx.args::<TestArgs>();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing required arg: name"));
    }

    #[test]
    fn context_args_optional_field_defaults_to_none() {
        let mut args = BTreeMap::new();
        args.insert("name".into(), serde_json::json!("test"));
        let ctx = make_context("default", args, None);
        let parsed = ctx.args::<TestArgs>().unwrap();
        assert_eq!(parsed.count, None);
        assert!(!parsed.verbose);
    }

    #[test]
    fn context_input_returns_some_then_none() {
        let mut ctx = make_context("default", BTreeMap::new(), Some("line1\nline2\n"));
        let input = ctx.input();
        assert!(input.is_some());
        let mut lines = Vec::new();
        for line in input.unwrap().lines() {
            lines.push(line.unwrap());
        }
        assert_eq!(lines, vec!["line1", "line2"]);

        // Second call returns None
        assert!(ctx.input().is_none());
    }

    #[test]
    fn context_input_returns_none_when_no_input() {
        let mut ctx = make_context("default", BTreeMap::new(), None);
        assert!(ctx.input().is_none());
    }

    #[test]
    fn context_input_or_empty_returns_empty_when_no_input() {
        let mut ctx = make_context("default", BTreeMap::new(), None);
        let reader = ctx.input_or_empty();
        let mut buf = String::new();
        reader.lines().for_each(|l| buf.push_str(&l.unwrap()));
        assert!(buf.is_empty());
    }

    #[test]
    fn context_input_or_empty_returns_data_when_present() {
        let mut ctx = make_context("default", BTreeMap::new(), Some("hello\n"));
        let reader = ctx.input_or_empty();
        let lines: Vec<_> = reader.lines().map(|l| l.unwrap()).collect();
        assert_eq!(lines, vec!["hello"]);
    }

    // ─── Protocol handler return mapping tests ───────────────────────
    // These test the handler -> (exit_code, error) mapping logic
    // without requiring the full runtime, by testing the pattern directly.

    #[test]
    fn protocol_handler_ok_maps_to_exit_code() {
        // Simulates what protocol() does with the handler result
        let result: Result<i32, String> = Ok(0);
        let (code, error) = match result {
            Ok(exit_code) => (exit_code, None),
            Err(e) => (1, Some(e.to_string())),
        };
        assert_eq!(code, 0);
        assert!(error.is_none());
    }

    #[test]
    fn protocol_handler_ok_nonzero_maps_to_exit_code() {
        let result: Result<i32, String> = Ok(2);
        let (code, error) = match result {
            Ok(exit_code) => (exit_code, None),
            Err(e) => (1, Some(e.to_string())),
        };
        assert_eq!(code, 2);
        assert!(error.is_none());
    }

    #[test]
    fn protocol_handler_err_maps_to_code_1_with_message() {
        let result: Result<i32, String> = Err("something failed".into());
        let (code, error) = match result {
            Ok(exit_code) => (exit_code, None),
            Err(e) => (1, Some(e.to_string())),
        };
        assert_eq!(code, 1);
        assert_eq!(error.unwrap(), "something failed");
    }

    // ─── Service Loop Tests ──────────────────────────────────────────

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn service_loop_invoke_over_tcp() {
        use crate::protocol::{Done, Invoke, Message, Output};
        use std::collections::BTreeMap;

        // Start a service listener on a random port
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let handle = std::thread::spawn(move || {
            let state = Arc::new(Mutex::new(Vec::<String>::new()));
            let handler = Arc::new(
                |s: &mut Vec<String>, inv: Invoke| -> Vec<serde_json::Value> {
                    s.push(inv.command.clone());
                    vec![serde_json::json!({"echoed": inv.command})]
                },
            );

            // Accept one connection
            let (stream, _) = listener.accept().unwrap();
            handle_service_connection(stream, &state, &handler);
        });

        // Give the listener thread a moment
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Connect and send INVOKE
        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
        let invoke = Message::Invoke(Invoke {
            version: 1,
            command: "test-cmd".into(),
            args: BTreeMap::new(),
            input: false,
        });
        let json = serde_json::to_string(&invoke).unwrap();
        writeln!(stream, "{}", json).unwrap();
        stream.flush().unwrap();

        // Read response
        let reader = BufReader::new(stream);
        let mut messages: Vec<Message> = Vec::new();
        for line in reader.lines() {
            let line = line.unwrap();
            if line.is_empty() {
                continue;
            }
            let msg: Message = serde_json::from_str(&line).unwrap();
            let is_done = matches!(&msg, Message::Done(_));
            messages.push(msg);
            if is_done {
                break;
            }
        }

        handle.join().unwrap();

        // Verify OUTPUT + DONE
        assert_eq!(messages.len(), 2);
        match &messages[0] {
            Message::Output(Output { record }) => {
                assert_eq!(record["echoed"], "test-cmd");
            }
            other => panic!("expected Output, got {:?}", other),
        }
        match &messages[1] {
            Message::Done(Done { exit_code, error }) => {
                assert_eq!(*exit_code, 0);
                assert!(error.is_none());
            }
            other => panic!("expected Done, got {:?}", other),
        }
    }

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn service_loop_health_check() {
        // Start a service listener on a random port
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let handle = std::thread::spawn(move || {
            let state = Arc::new(Mutex::new(()));
            let handler = Arc::new(
                |_: &mut (), _: crate::protocol::Invoke| -> Vec<serde_json::Value> { vec![] },
            );

            // Accept one connection (health check)
            let (stream, _) = listener.accept().unwrap();
            handle_service_connection(stream, &state, &handler);
        });

        std::thread::sleep(std::time::Duration::from_millis(50));

        // Send an HTTP GET /health
        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
        let request = "GET /health HTTP/1.0\r\nHost: localhost\r\n\r\n";
        stream.write_all(request.as_bytes()).unwrap();
        stream.flush().unwrap();

        // Read response
        let mut response = String::new();
        let mut reader = BufReader::new(stream);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => response.push_str(&line),
                Err(_) => break,
            }
        }

        handle.join().unwrap();

        assert!(response.contains("200 OK"), "got: {}", response);
        assert!(response.contains(r#"{"status":"ok"}"#), "got: {}", response);
    }
}
