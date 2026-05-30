use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use serde::Deserialize;
use zacor_protocol::DaemonRefusal;
use zacor_protocol::daemon_catalog::{InstalledPackageSummary, PackageDescriptor};
use zacor_protocol::daemon_invoke::{CommandInvocationRequest, InvocationEvent};

const DEFAULT_DAEMON_ADDR: &str = "127.0.0.1:19100";

pub struct ZrClient {
    addr: String,
}

impl ZrClient {
    pub fn new(addr: impl Into<String>) -> Self {
        Self { addr: addr.into() }
    }

    pub fn new_default() -> Self {
        Self::new(DEFAULT_DAEMON_ADDR)
    }

    pub fn list_packages(&self) -> Result<Vec<InstalledPackageSummary>, ZrClientError> {
        self.send_request(
            &serde_json::json!({"request": "list-packages"}),
            "list-packages",
        )
    }

    pub fn describe_package(&self, name: &str) -> Result<PackageDescriptor, ZrClientError> {
        self.send_request(
            &serde_json::json!({"request": "describe-package", "name": name}),
            "describe-package",
        )
    }

    pub fn invoke_command(
        &self,
        request: CommandInvocationRequest,
    ) -> Result<InvocationStream, ZrClientError> {
        let mut stream =
            TcpStream::connect(&self.addr).map_err(|error| ZrClientError::Connect {
                addr: self.addr.clone(),
                source: error,
            })?;

        let request_json = serde_json::to_string(&serde_json::json!({
            "request": "invoke-command",
            "invoke": request,
        }))
        .map_err(|error| ZrClientError::Serialize {
            request: "invoke-command".to_string(),
            source: error,
        })?;
        writeln!(stream, "{}", request_json).map_err(|error| ZrClientError::Write {
            request: "invoke-command".to_string(),
            source: error,
        })?;
        stream.flush().map_err(|error| ZrClientError::Write {
            request: "invoke-command".to_string(),
            source: error,
        })?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|error| ZrClientError::Read {
                request: "invoke-command".to_string(),
                source: error,
            })?;

        let response: DaemonEnvelope =
            serde_json::from_str(line.trim()).map_err(|error| ZrClientError::Parse {
                request: "invoke-command".to_string(),
                source: error,
            })?;

        if !response.ok {
            if let Some(refusal) = response.refusal {
                return Err(ZrClientError::DaemonRefusal(refusal));
            }
            return Err(ZrClientError::DaemonError(
                response
                    .error
                    .unwrap_or_else(|| "unknown daemon error".to_string()),
            ));
        }

        Ok(InvocationStream { reader })
    }

    fn send_request<T: for<'de> Deserialize<'de>>(
        &self,
        request: &serde_json::Value,
        request_name: &str,
    ) -> Result<T, ZrClientError> {
        let mut stream =
            TcpStream::connect(&self.addr).map_err(|error| ZrClientError::Connect {
                addr: self.addr.clone(),
                source: error,
            })?;

        let request_json =
            serde_json::to_string(request).map_err(|error| ZrClientError::Serialize {
                request: request_name.to_string(),
                source: error,
            })?;
        writeln!(stream, "{}", request_json).map_err(|error| ZrClientError::Write {
            request: request_name.to_string(),
            source: error,
        })?;
        stream.flush().map_err(|error| ZrClientError::Write {
            request: request_name.to_string(),
            source: error,
        })?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|error| ZrClientError::Read {
                request: request_name.to_string(),
                source: error,
            })?;

        let response: DaemonEnvelope =
            serde_json::from_str(line.trim()).map_err(|error| ZrClientError::Parse {
                request: request_name.to_string(),
                source: error,
            })?;

        if !response.ok {
            if let Some(refusal) = response.refusal {
                return Err(ZrClientError::DaemonRefusal(refusal));
            }
            return Err(ZrClientError::DaemonError(
                response
                    .error
                    .unwrap_or_else(|| "unknown daemon error".to_string()),
            ));
        }

        let result = response.result.ok_or(ZrClientError::MissingResult {
            request: request_name.to_string(),
        })?;
        serde_json::from_value(result).map_err(|error| ZrClientError::Decode {
            request: request_name.to_string(),
            source: error,
        })
    }
}

#[derive(Debug)]
pub struct InvocationStream {
    reader: BufReader<TcpStream>,
}

impl InvocationStream {
    pub fn next_event(&mut self) -> Result<Option<InvocationEvent>, ZrClientError> {
        loop {
            let mut line = String::new();
            let bytes = self
                .reader
                .read_line(&mut line)
                .map_err(|error| ZrClientError::ReadEvent { source: error })?;
            if bytes == 0 {
                return Ok(None);
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let event = serde_json::from_str(trimmed)
                .map_err(|error| ZrClientError::ParseEvent { source: error })?;
            return Ok(Some(event));
        }
    }
}

#[derive(Debug)]
pub enum ZrClientError {
    Connect {
        addr: String,
        source: std::io::Error,
    },
    Serialize {
        request: String,
        source: serde_json::Error,
    },
    Write {
        request: String,
        source: std::io::Error,
    },
    Read {
        request: String,
        source: std::io::Error,
    },
    Parse {
        request: String,
        source: serde_json::Error,
    },
    Decode {
        request: String,
        source: serde_json::Error,
    },
    ReadEvent {
        source: std::io::Error,
    },
    ParseEvent {
        source: serde_json::Error,
    },
    DaemonRefusal(DaemonRefusal),
    DaemonError(String),
    MissingResult {
        request: String,
    },
}

impl fmt::Display for ZrClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect { addr, source } => {
                write!(
                    formatter,
                    "failed to connect to daemon at {}: {}",
                    addr, source
                )
            }
            Self::Serialize { request, source } => {
                write!(
                    formatter,
                    "failed to serialize {} request: {}",
                    request, source
                )
            }
            Self::Write { request, source } => {
                write!(formatter, "failed to write {} request: {}", request, source)
            }
            Self::Read { request, source } => {
                write!(formatter, "failed to read {} response: {}", request, source)
            }
            Self::Parse { request, source } => {
                write!(
                    formatter,
                    "failed to parse {} response envelope: {}",
                    request, source
                )
            }
            Self::Decode { request, source } => {
                write!(
                    formatter,
                    "failed to decode {} response payload: {}",
                    request, source
                )
            }
            Self::ReadEvent { source } => {
                write!(formatter, "failed to read invocation event: {}", source)
            }
            Self::ParseEvent { source } => {
                write!(formatter, "failed to parse invocation event: {}", source)
            }
            Self::DaemonRefusal(refusal) => {
                write!(formatter, "daemon refused request: {:?}", refusal)
            }
            Self::DaemonError(message) => formatter.write_str(message),
            Self::MissingResult { request } => {
                write!(
                    formatter,
                    "daemon returned no result payload for {}",
                    request
                )
            }
        }
    }
}

impl std::error::Error for ZrClientError {}

#[derive(Debug, Deserialize)]
struct DaemonEnvelope {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    refusal: Option<DaemonRefusal>,
    #[serde(default)]
    result: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;
    use zacor_protocol::daemon_catalog::{
        ArgumentDescriptor, ArgumentType, CommandDescriptor, OutputCardinality, OutputDescriptor,
        OutputDisplay,
    };
    use zacor_protocol::daemon_invoke::{InvocationContext, InvocationMessageLevel};

    fn spawn_server(response: serde_json::Value) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(!line.trim().is_empty());
            writeln!(stream, "{}", response).unwrap();
            stream.flush().unwrap();
        });
        addr
    }

    fn spawn_stream_server(ack: serde_json::Value, events: Vec<serde_json::Value>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let request: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
            assert_eq!(request["request"], "invoke-command");
            writeln!(stream, "{}", ack).unwrap();
            for event in events {
                writeln!(stream, "{}", event).unwrap();
            }
            stream.flush().unwrap();
        });
        addr
    }

    fn invoke_request() -> CommandInvocationRequest {
        CommandInvocationRequest {
            package: "echo".into(),
            command: "default".into(),
            args: std::collections::BTreeMap::from([("text".into(), "hello".into())]),
            context: InvocationContext { cwd: ".".into() },
        }
    }

    #[test]
    fn list_packages_decodes_success_response() {
        let addr = spawn_server(serde_json::json!({
            "ok": true,
            "result": [
                {
                    "name": "echo",
                    "version": "0.2.0",
                    "active": true,
                    "description": "Echo text"
                }
            ]
        }));
        let client = ZrClient::new(addr);

        let packages = client.list_packages().unwrap();

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "echo");
        assert_eq!(packages[0].description.as_deref(), Some("Echo text"));
    }

    #[test]
    fn describe_package_decodes_nested_descriptor() {
        let descriptor = PackageDescriptor {
            name: "tool".into(),
            version: "1.0.0".into(),
            active: true,
            description: Some("Tool".into()),
            commands: std::collections::BTreeMap::from([(
                "default".into(),
                CommandDescriptor {
                    description: Some("Run tool".into()),
                    args: std::collections::BTreeMap::from([(
                        "path".into(),
                        ArgumentDescriptor {
                            arg_type: ArgumentType::Path,
                            required: true,
                            flag: None,
                            values: None,
                            rest: false,
                        },
                    )]),
                    commands: std::collections::BTreeMap::new(),
                    input: None,
                    output: Some(OutputDescriptor {
                        cardinality: OutputCardinality::One,
                        display: Some(OutputDisplay::Record),
                        field: None,
                        stream: false,
                        schema: None,
                    }),
                },
            )]),
        };
        let addr = spawn_server(serde_json::json!({
            "ok": true,
            "result": descriptor,
        }));
        let client = ZrClient::new(addr);

        let described = client.describe_package("tool").unwrap();

        assert_eq!(described.name, "tool");
        assert_eq!(
            described.commands["default"].args["path"].arg_type,
            ArgumentType::Path
        );
    }

    #[test]
    fn refusal_maps_to_daemon_refusal_error() {
        let addr = spawn_server(serde_json::json!({
            "ok": false,
            "refusal": {
                "kind": "package_not_found",
                "name": "missing"
            },
            "error": "package not found: missing"
        }));
        let client = ZrClient::new(addr);

        let error = client.describe_package("missing").unwrap_err();

        assert!(matches!(
            error,
            ZrClientError::DaemonRefusal(DaemonRefusal::PackageNotFound { name }) if name == "missing"
        ));
    }

    #[test]
    fn missing_result_maps_to_error() {
        let addr = spawn_server(serde_json::json!({"ok": true}));
        let client = ZrClient::new(addr);

        let error = client.list_packages().unwrap_err();

        assert!(matches!(error, ZrClientError::MissingResult { .. }));
    }

    #[test]
    fn invalid_payload_maps_to_decode_error() {
        let addr = spawn_server(serde_json::json!({
            "ok": true,
            "result": {"unexpected": true}
        }));
        let client = ZrClient::new(addr);

        let error = client.list_packages().unwrap_err();

        assert!(matches!(error, ZrClientError::Decode { .. }));
    }

    #[test]
    fn invoke_command_stream_decodes_events_in_order() {
        let addr = spawn_stream_server(
            serde_json::json!({"ok": true}),
            vec![
                serde_json::json!({"type": "output", "record": {"value": "hello"}}),
                serde_json::json!({"type": "progress", "fraction": 0.5}),
                serde_json::json!({"type": "message", "level": "info", "text": "running"}),
                serde_json::json!({"type": "done", "exit_code": 0}),
            ],
        );
        let client = ZrClient::new(addr);

        let mut stream = client.invoke_command(invoke_request()).unwrap();

        assert_eq!(
            stream.next_event().unwrap(),
            Some(InvocationEvent::Output {
                record: serde_json::json!({"value": "hello"})
            })
        );
        assert_eq!(
            stream.next_event().unwrap(),
            Some(InvocationEvent::Progress { fraction: 0.5 })
        );
        assert_eq!(
            stream.next_event().unwrap(),
            Some(InvocationEvent::Message {
                level: InvocationMessageLevel::Info,
                text: "running".into(),
            })
        );
        assert_eq!(
            stream.next_event().unwrap(),
            Some(InvocationEvent::Done {
                exit_code: 0,
                error: None,
            })
        );
        assert_eq!(stream.next_event().unwrap(), None);
    }

    #[test]
    fn invoke_command_refusal_before_stream_maps_to_error() {
        let addr = spawn_stream_server(
            serde_json::json!({
                "ok": false,
                "refusal": {"kind": "package_not_found", "name": "missing"},
                "error": "package not found: missing"
            }),
            Vec::new(),
        );
        let client = ZrClient::new(addr);

        let error = client.invoke_command(invoke_request()).unwrap_err();

        assert!(matches!(
            error,
            ZrClientError::DaemonRefusal(DaemonRefusal::PackageNotFound { name }) if name == "missing"
        ));
    }

    #[test]
    fn malformed_invocation_event_maps_to_parse_error() {
        let addr = spawn_stream_server(
            serde_json::json!({"ok": true}),
            vec![serde_json::json!({"type": "unknown"})],
        );
        let client = ZrClient::new(addr);
        let mut stream = client.invoke_command(invoke_request()).unwrap();

        let error = stream.next_event().unwrap_err();

        assert!(matches!(error, ZrClientError::ParseEvent { .. }));
    }
}
