use crate::capability::CapabilityRegistry;
use crate::package_definition::{CommandDefinition, PackageDefinition};
use crate::protocol::{Input, InvokePackageDone, InvokePackageOutput, Message};
use crate::router::PackageRouter;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::mpsc::{Receiver, RecvError, SendError, Sender};

pub trait Transport {
    fn send(&mut self, msg: &Message) -> Result<(), TransportError>;
    fn recv(&mut self) -> Result<Option<Message>, TransportError>;
}

pub trait OutputHandler {
    fn on_output(&mut self, record: &serde_json::Value);

    fn on_progress(&mut self, _fraction: f64) {}

    fn finish(&mut self) {}
}

pub trait InputSource {
    fn next_chunk(&mut self) -> Option<String>;
}

pub struct SessionConfig<'a> {
    pub invoke: &'a Message,
    pub send_invoke: bool,
    pub package_name: Option<&'a str>,
    pub package_definition: Option<&'a PackageDefinition>,
    pub command: &'a CommandDefinition,
    pub capabilities: &'a CapabilityRegistry,
    pub package_router: Option<&'a dyn PackageRouter>,
    pub output_handler: &'a mut dyn OutputHandler,
    pub input_source: Option<&'a mut dyn InputSource>,
    pub depth: usize,
    pub max_depth: usize,
}

#[derive(Debug)]
pub enum TransportError {
    Io(std::io::Error),
    Json(serde_json::Error),
    ChannelSend,
    ChannelRecv,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "io: {error}"),
            Self::Json(error) => write!(f, "json: {error}"),
            Self::ChannelSend => write!(f, "channel send failed"),
            Self::ChannelRecv => write!(f, "channel receive failed"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::ChannelSend | Self::ChannelRecv => None,
        }
    }
}

impl From<std::io::Error> for TransportError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for TransportError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug)]
pub enum SessionError {
    Transport(TransportError),
    Protocol(String),
    DepthExceeded,
    Io(std::io::Error),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(error) => write!(f, "transport: {error}"),
            Self::Protocol(message) => write!(f, "protocol: {message}"),
            Self::DepthExceeded => write!(f, "call depth exceeded"),
            Self::Io(error) => write!(f, "io: {error}"),
        }
    }
}

impl std::error::Error for SessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(error) => Some(error),
            Self::Protocol(_) | Self::DepthExceeded => None,
            Self::Io(error) => Some(error),
        }
    }
}

impl From<TransportError> for SessionError {
    fn from(error: TransportError) -> Self {
        Self::Transport(error)
    }
}

impl From<std::io::Error> for SessionError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub struct StdioTransport<R, W> {
    reader: R,
    writer: W,
}

impl<R, W> StdioTransport<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }
}

impl<R, W> Transport for StdioTransport<R, W>
where
    R: BufRead,
    W: Write,
{
    fn send(&mut self, msg: &Message) -> Result<(), TransportError> {
        let json = serde_json::to_string(msg)?;
        writeln!(self.writer, "{json}")?;
        self.writer.flush()?;
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<Message>, TransportError> {
        loop {
            let mut line = String::new();
            match self.reader.read_line(&mut line)? {
                0 => return Ok(None),
                _ if line.trim().is_empty() => continue,
                _ => match serde_json::from_str::<Message>(&line) {
                    Ok(message) => return Ok(Some(message)),
                    Err(_) => continue,
                },
            }
        }
    }
}

pub struct TcpTransport {
    inner: StdioTransport<BufReader<TcpStream>, TcpStream>,
}

impl TcpTransport {
    pub fn new(stream: TcpStream) -> Result<Self, std::io::Error> {
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Self {
            inner: StdioTransport::new(reader, stream),
        })
    }
}

impl Transport for TcpTransport {
    fn send(&mut self, msg: &Message) -> Result<(), TransportError> {
        self.inner.send(msg)
    }

    fn recv(&mut self) -> Result<Option<Message>, TransportError> {
        self.inner.recv()
    }
}

pub struct ChannelTransport {
    output_tx: Sender<Message>,
    input_rx: Receiver<Message>,
}

impl ChannelTransport {
    pub fn new(output_tx: Sender<Message>, input_rx: Receiver<Message>) -> Self {
        Self {
            output_tx,
            input_rx,
        }
    }
}

impl Transport for ChannelTransport {
    fn send(&mut self, msg: &Message) -> Result<(), TransportError> {
        self.output_tx
            .send(msg.clone())
            .map_err(|_: SendError<Message>| TransportError::ChannelSend)
    }

    fn recv(&mut self) -> Result<Option<Message>, TransportError> {
        match self.input_rx.recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(RecvError) => Ok(None),
        }
    }
}

pub fn run_session(
    transport: &mut dyn Transport,
    config: SessionConfig<'_>,
) -> Result<i32, SessionError> {
    let SessionConfig {
        invoke,
        send_invoke,
        package_name: _,
        package_definition,
        command: _,
        capabilities,
        package_router,
        output_handler,
        input_source,
        depth,
        max_depth,
    } = config;

    if max_depth != 0 && depth > max_depth {
        return Err(SessionError::DepthExceeded);
    }

    if send_invoke {
        transport.send(invoke)?;
    }

    let has_input = matches!(invoke, Message::Invoke(inv) if inv.input);
    if has_input
        && let Some(input_source) = input_source
    {
        while let Some(chunk) = input_source.next_chunk() {
            transport.send(&Message::Input(Input {
                data: chunk,
                eof: false,
            }))?;
        }
        transport.send(&Message::Input(Input {
            data: String::new(),
            eof: true,
        }))?;
    }

    loop {
        let Some(message) = transport.recv()? else {
            output_handler.finish();
            return Ok(1);
        };

        match message {
            Message::Output(output) => output_handler.on_output(&output.record),
            Message::Progress(progress) => output_handler.on_progress(progress.fraction),
            Message::CapabilityReq(req) => {
                transport.send(&Message::CapabilityRes(capabilities.dispatch(&req)))?;
            }
            Message::InvokePackage(invoke_package) => {
                let outcome = if max_depth != 0 && depth + 1 > max_depth {
                    crate::router::InvocationOutcome::failure("call depth exceeded")
                } else if let Some(package_router) = package_router {
                    package_router.invoke(
                        package_definition,
                        &invoke_package.package,
                        &invoke_package.command,
                        &invoke_package.args,
                        depth + 1,
                        max_depth,
                        &mut |record| {
                            transport
                                .send(&Message::InvokePackageOutput(InvokePackageOutput {
                                    id: invoke_package.id,
                                    record,
                                }))
                                .map_err(|error| error.to_string())
                        },
                    )
                } else {
                    crate::router::InvocationOutcome::failure(
                        "cross-package invocation is unavailable in this session",
                    )
                };

                transport.send(&Message::InvokePackageDone(InvokePackageDone {
                    id: invoke_package.id,
                    exit_code: outcome.exit_code,
                    error: outcome.error,
                }))?;
            }
            Message::Done(done) => {
                if let Some(error) = done.error {
                    eprintln!("error: {error}");
                }
                output_handler.finish();
                return Ok(done.exit_code);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilityProvider, CapabilityRegistry};
    use crate::protocol::{CapabilityError, CapabilityResult, CapabilityRes, Output};
    use crate::router::{InvocationOutcome, PackageRouter};
    use serde_json::json;
    use std::sync::Arc;

    struct TestOutputHandler {
        records: Vec<serde_json::Value>,
        progress: Vec<f64>,
        finished: bool,
    }

    impl OutputHandler for TestOutputHandler {
        fn on_output(&mut self, record: &serde_json::Value) {
            self.records.push(record.clone());
        }

        fn on_progress(&mut self, fraction: f64) {
            self.progress.push(fraction);
        }

        fn finish(&mut self) {
            self.finished = true;
        }
    }

    struct VecInputSource {
        chunks: std::vec::IntoIter<String>,
    }

    impl InputSource for VecInputSource {
        fn next_chunk(&mut self) -> Option<String> {
            self.chunks.next()
        }
    }

    struct EchoProvider;

    impl CapabilityProvider for EchoProvider {
        fn domain(&self) -> &str {
            "fs"
        }

        fn handle(&self, op: &str, _params: &serde_json::Value) -> Result<serde_json::Value, CapabilityError> {
            Ok(json!({"op": op}))
        }
    }

    struct TestRouter;

    impl PackageRouter for TestRouter {
        fn invoke(
            &self,
            _caller: Option<&PackageDefinition>,
            package: &str,
            command: &str,
            _args: &std::collections::BTreeMap<String, String>,
            _depth: usize,
            _max_depth: usize,
            on_output: &mut dyn FnMut(serde_json::Value) -> Result<(), String>,
        ) -> InvocationOutcome {
            let _ = on_output(json!({"package": package, "command": command}));
            InvocationOutcome::success(0)
        }
    }

    #[test]
    fn stdio_transport_round_trips_messages() {
        let input = concat!(
            r#"{"type":"output","record":{"hello":"world"}}"#,
            "\n",
            r#"{"type":"done","exit_code":0}"#,
            "\n",
        );
        let mut transport = StdioTransport::new(std::io::Cursor::new(input.as_bytes()), Vec::new());

        let first = transport.recv().unwrap().unwrap();
        let second = transport.recv().unwrap().unwrap();
        let third = transport.recv().unwrap();

        assert!(matches!(first, Message::Output(_)));
        assert!(matches!(second, Message::Done(_)));
        assert!(third.is_none());

        transport
            .send(&Message::Output(Output {
                record: json!({"ok": true}),
            }))
            .unwrap();
        let written = String::from_utf8(transport.writer).unwrap();
        assert!(written.contains("\"type\":\"output\""));
    }

    #[test]
    fn run_session_handles_output_progress_capabilities_and_done() {
        let input = concat!(
            r#"{"type":"output","record":{"hello":"world"}}"#,
            "\n",
            r#"{"type":"progress","fraction":0.5}"#,
            "\n",
            r#"{"type":"capability_req","id":9,"domain":"fs","op":"exists","params":{}}"#,
            "\n",
            r#"{"type":"done","exit_code":3}"#,
            "\n",
        );
        let mut transport = StdioTransport::new(std::io::Cursor::new(input.as_bytes()), Vec::new());
        let mut registry = CapabilityRegistry::new();
        registry.register(Arc::new(EchoProvider)).unwrap();
        let invoke = Message::Invoke(crate::protocol::Invoke::from_str_args("default", &std::collections::BTreeMap::new(), false));
        let command = CommandDefinition::default();
        let mut output_handler = TestOutputHandler {
            records: Vec::new(),
            progress: Vec::new(),
            finished: false,
        };

        let exit_code = run_session(
            &mut transport,
            SessionConfig {
                invoke: &invoke,
                send_invoke: true,
                package_name: None,
                package_definition: None,
                command: &command,
                capabilities: &registry,
                package_router: None,
                output_handler: &mut output_handler,
                input_source: None,
                depth: 0,
                max_depth: 0,
            },
        )
        .unwrap();

        assert_eq!(exit_code, 3);
        assert_eq!(output_handler.records, vec![json!({"hello": "world"})]);
        assert_eq!(output_handler.progress, vec![0.5]);
        assert!(output_handler.finished);

        let written = String::from_utf8(transport.writer).unwrap();
        assert!(written.contains("\"type\":\"invoke\""));
        assert!(written.contains("\"type\":\"capability_res\""));
        let capability_res: CapabilityRes = serde_json::from_str(
            written
                .lines()
                .find(|line| line.contains("\"type\":\"capability_res\""))
                .unwrap(),
        )
        .unwrap();
        match capability_res.result {
            CapabilityResult::Ok { data } => assert_eq!(data["op"], "exists"),
            CapabilityResult::Error { .. } => panic!("expected ok"),
        }
    }

    #[test]
    fn run_session_forwards_input_chunks() {
        let input = concat!(r#"{"type":"done","exit_code":0}"#, "\n");
        let mut transport = StdioTransport::new(std::io::Cursor::new(input.as_bytes()), Vec::new());
        let registry = CapabilityRegistry::new();
        let invoke = Message::Invoke(crate::protocol::Invoke::from_str_args(
            "default",
            &std::collections::BTreeMap::new(),
            true,
        ));
        let command = CommandDefinition::default();
        let mut output_handler = TestOutputHandler {
            records: Vec::new(),
            progress: Vec::new(),
            finished: false,
        };
        let mut input_source = VecInputSource {
            chunks: vec!["hello\n".into(), "world\n".into()].into_iter(),
        };

        run_session(
            &mut transport,
            SessionConfig {
                invoke: &invoke,
                send_invoke: true,
                package_name: None,
                package_definition: None,
                command: &command,
                capabilities: &registry,
                package_router: None,
                output_handler: &mut output_handler,
                input_source: Some(&mut input_source),
                depth: 0,
                max_depth: 0,
            },
        )
        .unwrap();

        let written = String::from_utf8(transport.writer).unwrap();
        assert!(written.contains("\"type\":\"input\",\"data\":\"hello\\n\",\"eof\":false"));
        assert!(written.contains("\"type\":\"input\",\"data\":\"world\\n\",\"eof\":false"));
        assert!(written.contains("\"type\":\"input\",\"data\":\"\",\"eof\":true"));
    }

    #[test]
    fn channel_transport_round_trips() {
        let (out_tx, out_rx) = std::sync::mpsc::channel();
        let (in_tx, in_rx) = std::sync::mpsc::channel();
        let mut transport = ChannelTransport::new(out_tx, in_rx);
        let msg = Message::Done(crate::protocol::Done {
            exit_code: 0,
            error: None,
        });

        transport.send(&msg).unwrap();
        assert!(matches!(out_rx.recv().unwrap(), Message::Done(_)));

        in_tx.send(msg).unwrap();
        assert!(matches!(transport.recv().unwrap(), Some(Message::Done(_))));
    }

    #[test]
    fn run_session_routes_invoke_package_messages() {
        let input = concat!(
            r#"{"type":"invoke_package","id":5,"package":"cat","command":"default","args":{}}"#,
            "\n",
            r#"{"type":"done","exit_code":0}"#,
            "\n",
        );
        let mut transport = StdioTransport::new(std::io::Cursor::new(input.as_bytes()), Vec::new());
        let registry = CapabilityRegistry::new();
        let invoke = Message::Invoke(crate::protocol::Invoke::from_str_args(
            "default",
            &std::collections::BTreeMap::new(),
            false,
        ));
        let command = CommandDefinition::default();
        let mut output_handler = TestOutputHandler {
            records: Vec::new(),
            progress: Vec::new(),
            finished: false,
        };

        let exit_code = run_session(
            &mut transport,
            SessionConfig {
                invoke: &invoke,
                send_invoke: true,
                package_name: None,
                package_definition: None,
                command: &command,
                capabilities: &registry,
                package_router: Some(&TestRouter),
                output_handler: &mut output_handler,
                input_source: None,
                depth: 0,
                max_depth: 8,
            },
        )
        .unwrap();

        assert_eq!(exit_code, 0);
        let written = String::from_utf8(transport.writer).unwrap();
        assert!(written.contains("\"type\":\"invoke_package_output\""));
        assert!(written.contains("\"type\":\"invoke_package_done\""));
    }
}
