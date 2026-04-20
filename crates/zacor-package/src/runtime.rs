//! Protocol runtime — manages bidirectional JSONL communication between
//! the module and the hosting runtime (zr CLI, HTTP server, etc.).
//!
//! Single-threaded demux. `capability_call` and `InputReader` share the
//! stdin reader through a `Mutex`. While a capability call is blocked
//! waiting for its matching response, interleaved `INPUT` messages are
//! buffered so that subsequent `InputReader` reads see them in order.
//! This design works identically on native and `wasm32-wasi*` targets,
//! where threads are unavailable.

use crate::protocol::*;
use std::collections::VecDeque;
use std::io::{self, BufRead, BufWriter, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

struct Runtime {
    reader: Mutex<Box<dyn BufRead + Send>>,
    writer: Mutex<BufWriter<Box<dyn Write + Send>>>,
    input_buffer: Mutex<VecDeque<Input>>,
    id_counter: AtomicU64,
}

/// Opaque handle returned by `init_invoke()` and consumed by `start_reader()`.
/// Retained for API compatibility; the single-threaded demux needs no
/// per-call state beyond the runtime singleton.
pub struct ReaderHandle {
    _private: (),
}

/// Phase 1: Read INVOKE from stdin, set up runtime state.
pub(crate) fn init_invoke() -> io::Result<(Invoke, Option<InputReader>, ReaderHandle)> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    init_invoke_with_io(io::BufReader::new(stdin), stdout)
}

/// Phase 1 with custom IO (for testing).
pub(crate) fn init_invoke_with_io(
    mut reader: impl BufRead + Send + 'static,
    writer: impl Write + Send + 'static,
) -> io::Result<(Invoke, Option<InputReader>, ReaderHandle)> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let msg: Message = serde_json::from_str(line.trim()).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected INVOKE: {}", e),
        )
    })?;
    let invoke = match msg {
        Message::Invoke(inv) => inv,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "first message must be INVOKE",
            ));
        }
    };

    let has_input = invoke.input;

    let runtime = Runtime {
        reader: Mutex::new(Box::new(reader)),
        writer: Mutex::new(BufWriter::new(Box::new(writer))),
        input_buffer: Mutex::new(VecDeque::new()),
        id_counter: AtomicU64::new(1),
    };

    let _ = RUNTIME.set(runtime);

    let input_reader = if has_input {
        Some(InputReader::new())
    } else {
        None
    };

    Ok((invoke, input_reader, ReaderHandle { _private: () }))
}

/// Phase 2: No-op under single-threaded demux. Kept for API compatibility —
/// the reader is drained lazily by `capability_call` and `InputReader` as
/// they need messages.
pub(crate) fn start_reader(_handle: ReaderHandle) -> io::Result<()> {
    Ok(())
}

/// Combined init (for callers that don't need the two-phase split).
#[allow(dead_code)]
pub(crate) fn init() -> io::Result<(Invoke, Option<InputReader>)> {
    let (invoke, input_reader, handle) = init_invoke()?;
    start_reader(handle)?;
    Ok((invoke, input_reader))
}

fn get_runtime() -> io::Result<&'static Runtime> {
    RUNTIME.get().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotConnected,
            "protocol runtime not initialized",
        )
    })
}

/// Send a protocol message to the runtime (via stdout).
pub(crate) fn send_message(msg: &Message) -> io::Result<()> {
    let rt = get_runtime()?;
    let mut json =
        serde_json::to_string(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    json.push('\n');
    let mut writer = rt.writer.lock().unwrap();
    writer.write_all(json.as_bytes())?;
    writer.flush()
}

/// Read the next valid protocol message from the runtime's reader.
/// Returns `Ok(None)` on EOF. Empty lines and unparseable JSON are skipped
/// per the protocol's forward-compat rule ("ignore unknown message types").
fn read_next_message(rt: &Runtime) -> io::Result<Option<Message>> {
    let mut reader = rt.reader.lock().unwrap();
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            return Ok(None);
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<Message>(trimmed) {
            Ok(msg) => return Ok(Some(msg)),
            Err(_) => continue,
        }
    }
}

/// Send a capability request and block until the response arrives.
/// INPUT messages that arrive while waiting are buffered for later
/// consumption by `InputReader`.
pub(crate) fn capability_call(
    domain: &str,
    op: &str,
    params: serde_json::Value,
) -> io::Result<serde_json::Value> {
    let rt = get_runtime()?;
    let id = rt.id_counter.fetch_add(1, Ordering::Relaxed);

    let msg = Message::CapabilityReq(CapabilityReq {
        id,
        domain: domain.to_string(),
        op: op.to_string(),
        params,
    });
    send_message(&msg)?;

    loop {
        let msg = read_next_message(rt)?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "protocol channel closed while waiting for capability response",
            )
        })?;

        match msg {
            Message::CapabilityRes(res) if res.id == id => {
                return match res.result {
                    CapabilityResult::Ok { data } => Ok(data),
                    CapabilityResult::Error { error } => Err(error.to_io()),
                };
            }
            Message::Input(input) => {
                rt.input_buffer.lock().unwrap().push_back(input);
            }
            _ => {}
        }
    }
}

/// Read the next INVOKE message from the runtime's reader. Skips any
/// other message types per the protocol's forward-compat rule. Returns
/// `Ok(None)` on EOF — callers use this to terminate service loops.
///
/// Must be called only after `init_invoke()` has established the runtime
/// (the runtime's reader is the protocol stdin pipe).
#[cfg(target_family = "wasm")]
pub(crate) fn read_next_invoke() -> io::Result<Option<Invoke>> {
    let rt = get_runtime()?;
    loop {
        match read_next_message(rt)? {
            None => return Ok(None),
            Some(Message::Invoke(inv)) => return Ok(Some(inv)),
            Some(_) => continue,
        }
    }
}

/// Send a progress report (fire-and-forget, errors silently ignored).
pub(crate) fn send_progress(fraction: f64) {
    let _ = send_message(&Message::Progress(Progress { fraction }));
}

/// Send an output record.
pub(crate) fn send_output(record: serde_json::Value) -> io::Result<()> {
    send_message(&Message::Output(Output { record }))
}

/// Send a done message.
pub(crate) fn send_done(exit_code: i32, error: Option<String>) -> io::Result<()> {
    send_message(&Message::Done(Done { exit_code, error }))
}

// ─── InputReader ─────────────────────────────────────────────────────

/// Reader for INPUT messages, implementing BufRead. Pulls INPUT messages
/// lazily from the runtime's reader, first draining any messages
/// buffered by `capability_call`.
pub struct InputReader {
    buffer: Vec<u8>,
    pos: usize,
    done: bool,
}

impl InputReader {
    fn new() -> Self {
        InputReader {
            buffer: Vec::new(),
            pos: 0,
            done: false,
        }
    }

    fn pull_next(&mut self) -> io::Result<()> {
        if self.pos < self.buffer.len() || self.done {
            return Ok(());
        }

        let rt = get_runtime()?;

        if let Some(input) = rt.input_buffer.lock().unwrap().pop_front() {
            self.ingest(input);
            return Ok(());
        }

        loop {
            let msg = read_next_message(rt)?.ok_or_else(|| {
                io::Error::new(io::ErrorKind::UnexpectedEof, "input channel closed")
            })?;
            match msg {
                Message::Input(input) => {
                    self.ingest(input);
                    return Ok(());
                }
                _ => continue,
            }
        }
    }

    fn ingest(&mut self, input: Input) {
        if input.eof {
            self.done = true;
        }
        self.buffer = input.data.into_bytes();
        self.pos = 0;
    }
}

impl io::Read for InputReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.pos < self.buffer.len() {
                let available = &self.buffer[self.pos..];
                let to_copy = available.len().min(buf.len());
                buf[..to_copy].copy_from_slice(&available[..to_copy]);
                self.pos += to_copy;
                return Ok(to_copy);
            }

            if self.done {
                return Ok(0);
            }

            self.pull_next()?;

            if self.done && self.buffer.is_empty() {
                return Ok(0);
            }
        }
    }
}

impl io::BufRead for InputReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.pos >= self.buffer.len() && !self.done {
            self.pull_next()?;
        }
        Ok(&self.buffer[self.pos..])
    }

    fn consume(&mut self, amt: usize) {
        self.pos += amt;
    }
}
