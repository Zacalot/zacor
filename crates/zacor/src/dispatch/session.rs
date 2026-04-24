use crate::error::*;
use crate::package_definition::{CommandDefinition, InlineInputFallback, OutputDeclaration, PackageDefinition};
use crate::render::RenderMode;
use serde_json::json;
use std::io::{BufRead, BufWriter, IsTerminal, StdoutLock, Write};
use serde_json::Value;
use zacor_host::capability::CapabilityRegistry;
use zacor_host::router::PackageRouter;
use zacor_host::session::{
    InputSource, OutputHandler, SessionConfig, StdioTransport, run_session,
};
use zacor_package::protocol::Message;

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

struct RenderingOutputHandler<'a> {
    output: &'a Option<OutputDeclaration>,
    render_mode: Option<RenderMode>,
    streaming: bool,
    is_tty: bool,
    stdout_writer: BufWriter<StdoutLock<'a>>,
    records: Vec<Value>,
    streaming_started: bool,
}

impl<'a> RenderingOutputHandler<'a> {
    fn new(output_mode: OutputMode, command: &'a CommandDefinition) -> Self {
        let is_tty = std::io::stdout().is_terminal();
        let render_mode = resolve_render_mode(output_mode, &command.output, is_tty);
        let stdout_handle = std::io::stdout();

        Self {
            output: &command.output,
            render_mode,
            streaming: command.output.as_ref().is_some_and(|output| output.stream),
            is_tty,
            stdout_writer: BufWriter::new(stdout_handle.lock()),
            records: Vec::new(),
            streaming_started: false,
        }
    }
}

impl OutputHandler for RenderingOutputHandler<'_> {
    fn on_output(&mut self, record: &Value) {
        if let Some(render_mode) = self.render_mode
            && self.streaming
        {
            if !self.streaming_started {
                if let Some(output_decl) = self.output {
                    crate::render::render_streaming_header(
                        output_decl,
                        render_mode,
                        &mut self.stdout_writer,
                    );
                }
                self.streaming_started = true;
            }
            if let Some(output_decl) = self.output {
                crate::render::render_streaming_row(
                    record,
                    output_decl,
                    render_mode,
                    &mut self.stdout_writer,
                );
            }
        } else if self.render_mode.is_some() {
            self.records.push(record.clone());
        } else {
            let json = serde_json::to_string(record).unwrap_or_default();
            let _ = writeln!(self.stdout_writer, "{json}");
            let _ = self.stdout_writer.flush();
        }
    }

    fn on_progress(&mut self, fraction: f64) {
        if self.is_tty {
            render_progress(fraction);
        }
    }

    fn finish(&mut self) {
        if self.is_tty {
            eprint!("\r\x1b[K");
        }

        if let Some(render_mode) = self.render_mode
            && !self.streaming
            && !self.records.is_empty()
            && let Some(output_decl) = self.output
        {
            crate::render::render_batch(
                &self.records,
                output_decl,
                render_mode,
                &mut self.stdout_writer,
            );
        }
        let _ = self.stdout_writer.flush();
    }
}

struct StdinInputSource {
    reader: std::io::BufReader<std::io::Stdin>,
    line: String,
}

impl StdinInputSource {
    fn new() -> Self {
        Self {
            reader: std::io::BufReader::new(std::io::stdin()),
            line: String::new(),
        }
    }
}

struct InlineInputSource {
    chunk: Option<String>,
}

impl InlineInputSource {
    fn from_invoke_args(invoke_msg: &Message, fallback: InlineInputFallback) -> Option<Self> {
        let Message::Invoke(invoke) = invoke_msg else {
            return None;
        };

        match fallback {
            InlineInputFallback::StringValue => invoke.args.get("value").map(|value| Self {
                chunk: Some(format!("{}\n", json!({"value": value}))),
            }),
        }
    }
}

impl InputSource for InlineInputSource {
    fn next_chunk(&mut self) -> Option<String> {
        self.chunk.take()
    }
}

impl InputSource for StdinInputSource {
    fn next_chunk(&mut self) -> Option<String> {
        self.line.clear();
        match self.reader.read_line(&mut self.line) {
            Ok(0) => None,
            Ok(_) => Some(self.line.clone()),
            Err(_) => None,
        }
    }
}

pub(crate) struct CallbackOutputHandler<'a> {
    on_record: &'a mut dyn FnMut(Value) -> std::result::Result<(), String>,
    error: Option<String>,
}

impl OutputHandler for CallbackOutputHandler<'_> {
    fn on_output(&mut self, record: &Value) {
        if self.error.is_none()
            && let Err(error) = (self.on_record)(record.clone())
        {
            self.error = Some(error);
        }
    }
}

impl CallbackOutputHandler<'_> {
    pub(crate) fn new(
        on_record: &mut dyn FnMut(Value) -> std::result::Result<(), String>,
    ) -> CallbackOutputHandler<'_> {
        CallbackOutputHandler {
            on_record,
            error: None,
        }
    }

    pub(crate) fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }
}

pub(crate) fn run_protocol_session_with_handler<'a>(
    reader: impl BufRead,
    writer: impl Write + Send + 'static,
    invoke_msg: &'a Message,
    send_invoke: bool,
    package_definition: &'a PackageDefinition,
    command: &'a CommandDefinition,
    capabilities: &'a CapabilityRegistry,
    package_router: Option<&'a dyn PackageRouter>,
    output_handler: &'a mut dyn OutputHandler,
    input_source: Option<&'a mut dyn InputSource>,
    depth: usize,
    max_depth: usize,
) -> Result<i32> {
    let mut transport = StdioTransport::new(reader, writer);
    run_session(
        &mut transport,
        SessionConfig {
            invoke: invoke_msg,
            send_invoke,
            package_name: Some(&package_definition.name),
            package_definition: Some(package_definition),
            command,
            capabilities,
            package_router,
            output_handler,
            input_source,
            depth,
            max_depth,
        },
    )
    .map_err(anyhow::Error::from)
}

/// Run a protocol session over generic reader/writer.
/// Used by both command-mode (child stdio) and service-mode (TCP) dispatch.
pub(crate) fn run_protocol_session(
    reader: impl BufRead,
    writer: impl Write + Send + 'static,
    invoke_msg: &Message,
    package_definition: &PackageDefinition,
    command: &CommandDefinition,
    output_mode: OutputMode,
    capabilities: &CapabilityRegistry,
    package_router: Option<&dyn PackageRouter>,
    depth: usize,
    max_depth: usize,
) -> Result<i32> {
    let mut output_handler = RenderingOutputHandler::new(output_mode, command);
    let mut inline_input_source = command
        .inline_input_fallback
        .and_then(|fallback| InlineInputSource::from_invoke_args(invoke_msg, fallback));
    let mut stdin_input_source = (command.input.is_some()
        && inline_input_source.is_none()
        && !std::io::stdin().is_terminal())
    .then(StdinInputSource::new);
    let input_source = inline_input_source
        .as_mut()
        .map(|source| source as &mut dyn InputSource)
        .or_else(|| {
            stdin_input_source
                .as_mut()
                .map(|source| source as &mut dyn InputSource)
        });

    run_protocol_session_with_handler(
        reader,
        writer,
        invoke_msg,
        true,
        package_definition,
        command,
        capabilities,
        package_router,
        &mut output_handler,
        input_source,
        depth,
        max_depth,
    )
}

/// Render a progress bar on stderr (in-place update).
fn render_progress(fraction: f64) {
    let clamped = fraction.clamp(0.0, 1.0);
    let pct = (clamped * 100.0) as u32;
    let filled = (clamped * 20.0) as usize;
    let bar: String = "█".repeat(filled) + &"░".repeat(20 - filled);
    eprint!("\r{} {:>3}%", bar, pct);
}
