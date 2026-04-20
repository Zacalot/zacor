use crate::error::*;
use crate::package_definition::{CommandDefinition, OutputDeclaration};
use crate::render::RenderMode;
use std::io::{BufRead, BufWriter, IsTerminal, Write};
use std::sync::{Arc, Mutex};
use zacor_package::protocol::{self as proto, Message};

use super::OutputMode;

pub(super) fn resolve_render_mode(
    mode: OutputMode,
    output: &Option<OutputDeclaration>,
    is_tty: bool,
) -> Option<RenderMode> {
    if output.is_none() {
        return None;
    }

    match mode {
        OutputMode::Auto => is_tty.then_some(RenderMode::Rich),
        OutputMode::Text => Some(RenderMode::Rich),
        OutputMode::Plain => Some(RenderMode::Plain),
        OutputMode::Json => None,
    }
}

/// Send a protocol message to a shared writer.
fn send_message(
    writer: &Arc<Mutex<BufWriter<Box<dyn Write + Send>>>>,
    msg: &Message,
) -> Result<()> {
    let json = serde_json::to_string(msg).context("failed to serialize protocol message")?;
    let mut w = writer.lock().unwrap();
    writeln!(w, "{}", json).context("failed to write to module")?;
    w.flush().context("failed to flush module writer")
}

/// Forward the dispatcher's stdin as INPUT messages to the module.
/// Uses line-by-line reading to avoid splitting multi-byte UTF-8 sequences
/// at buffer boundaries. Correct for text and jsonl input types.
fn forward_stdin_as_input(writer: Arc<Mutex<BufWriter<Box<dyn Write + Send>>>>) {
    let stdin = std::io::stdin();
    let mut reader = std::io::BufReader::new(stdin.lock());
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(_) => break,
        }
        let msg = Message::Input(proto::Input {
            data: line.clone(),
            eof: false,
        });
        if send_message(&writer, &msg).is_err() {
            break;
        }
    }
    let eof = Message::Input(proto::Input {
        data: String::new(),
        eof: true,
    });
    let _ = send_message(&writer, &eof);
}

/// Run a protocol session over generic reader/writer.
/// Used by both command-mode (child stdio) and service-mode (TCP) dispatch.
pub(crate) fn run_protocol_session(
    reader: impl BufRead,
    writer: impl Write + Send + 'static,
    invoke_msg: &Message,
    command: &CommandDefinition,
    output_mode: OutputMode,
) -> Result<i32> {
    let has_input = match invoke_msg {
        Message::Invoke(inv) => inv.input,
        _ => false,
    };

    // Shared writer for module (input thread + capability responses)
    let module_writer: Arc<Mutex<BufWriter<Box<dyn Write + Send>>>> =
        Arc::new(Mutex::new(BufWriter::new(Box::new(writer))));
    send_message(&module_writer, invoke_msg)?;

    // Forward stdin as INPUT messages if the command declares input and stdin is piped
    if has_input {
        let w = module_writer.clone();
        std::thread::Builder::new()
            .name("zr-input-fwd".into())
            .spawn(move || forward_stdin_as_input(w))
            .context("failed to spawn input forwarding thread")?;
    }

    // Protocol message loop
    let is_tty = std::io::stdout().is_terminal();
    let render_mode = resolve_render_mode(output_mode, &command.output, is_tty);
    let streaming = command.output.as_ref().is_some_and(|o| o.stream);

    let mut records: Vec<serde_json::Value> = Vec::new();
    let mut exit_code: Option<i32> = None;
    let mut streaming_started = false;

    // Set up stdout for rendering/output
    let stdout_handle = std::io::stdout();
    let mut stdout_writer = BufWriter::new(stdout_handle.lock());

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.is_empty() {
            continue;
        }

        let msg: Message = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(_) => continue, // Ignore unknown message types per spec
        };

        match msg {
            Message::Output(output) => {
                if let Some(render_mode) = render_mode
                    && streaming
                {
                    // Streaming render: emit each row as it arrives
                    if !streaming_started {
                        if let Some(output_decl) = &command.output {
                            crate::render::render_streaming_header(
                                output_decl,
                                render_mode,
                                &mut stdout_writer,
                            );
                        }
                        streaming_started = true;
                    }
                    if let Some(output_decl) = &command.output {
                        crate::render::render_streaming_row(
                            &output.record,
                            output_decl,
                            render_mode,
                            &mut stdout_writer,
                        );
                    }
                } else if render_mode.is_some() {
                    // Batch: collect for render at end
                    records.push(output.record);
                } else {
                    // Raw JSONL (piped or --json): output just the record payload
                    let json = serde_json::to_string(&output.record).unwrap_or_default();
                    let _ = writeln!(stdout_writer, "{}", json);
                    let _ = stdout_writer.flush();
                }
            }
            Message::Progress(progress) => {
                if is_tty {
                    render_progress(progress.fraction);
                }
            }
            Message::CapabilityReq(req) => {
                let res = crate::capability_provider::handle(&req);
                if send_message(&module_writer, &Message::CapabilityRes(res)).is_err() {
                    break;
                }
            }
            Message::Done(done) => {
                if let Some(ref error) = done.error {
                    eprintln!("error: {}", error);
                }
                exit_code = Some(done.exit_code);
                break;
            }
            _ => {} // Ignore unexpected messages
        }
    }

    // Clear progress line if we rendered any
    if is_tty {
        eprint!("\r\x1b[K");
    }

    // Batch render collected records
    if let Some(render_mode) = render_mode
        && !streaming
        && !records.is_empty()
    {
        if let Some(output_decl) = &command.output {
            crate::render::render_batch(&records, output_decl, render_mode, &mut stdout_writer);
        }
    }
    let _ = stdout_writer.flush();

    Ok(exit_code.unwrap_or(1))
}

/// Render a progress bar on stderr (in-place update).
fn render_progress(fraction: f64) {
    let clamped = fraction.clamp(0.0, 1.0);
    let pct = (clamped * 100.0) as u32;
    let filled = (clamped * 20.0) as usize;
    let bar: String = "█".repeat(filled) + &"░".repeat(20 - filled);
    eprint!("\r{} {:>3}%", bar, pct);
}
