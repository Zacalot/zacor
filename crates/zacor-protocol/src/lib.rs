//! Universal module protocol for bidirectional communication between runtime and modules.
//!
//! Seven message types over any bidirectional JSONL stream:
//! - INVOKE: runtime -> module, start execution
//! - OUTPUT: module -> runtime, result record
//! - DONE: module -> runtime, execution complete
//! - INPUT: runtime -> module, streaming input data
//! - PROGRESS: module -> runtime, progress report
//! - CAPABILITY_REQ: module -> runtime, capability request
//! - CAPABILITY_RES: runtime -> module, capability response

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const BASE64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode bytes to base64 string.
pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);

    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);

        out.push(BASE64_TABLE[(b0 >> 2) as usize] as char);
        out.push(BASE64_TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);

        if chunk.len() > 1 {
            out.push(BASE64_TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }

        if chunk.len() > 2 {
            out.push(BASE64_TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            out.push('=');
        }
    }

    out
}

/// Decode base64 string to bytes.
pub fn base64_decode(s: &str) -> Result<Vec<u8>, std::io::Error> {
    let bytes = s.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid base64 length",
        ));
    }

    let mut out = Vec::with_capacity((bytes.len() / 4) * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().rev().take_while(|&&b| b == b'=').count();
        let vals = [
            decode_base64_byte(chunk[0])?,
            decode_base64_byte(chunk[1])?,
            if chunk[2] == b'=' { 0 } else { decode_base64_byte(chunk[2])? },
            if chunk[3] == b'=' { 0 } else { decode_base64_byte(chunk[3])? },
        ];

        out.push((vals[0] << 2) | (vals[1] >> 4));
        if pad < 2 {
            out.push(((vals[1] & 0b0000_1111) << 4) | (vals[2] >> 2));
        }
        if pad < 1 {
            out.push(((vals[2] & 0b0000_0011) << 6) | vals[3]);
        }
    }

    Ok(out)
}

fn decode_base64_byte(byte: u8) -> Result<u8, std::io::Error> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid base64 byte: {byte}"),
        )),
    }
}

/// Top-level protocol message, internally tagged by `type`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Invoke(Invoke),
    Output(Output),
    Done(Done),
    Input(Input),
    Progress(Progress),
    CapabilityReq(CapabilityReq),
    CapabilityRes(CapabilityRes),
    InvokePackage(InvokePackage),
    InvokePackageOutput(InvokePackageOutput),
    InvokePackageDone(InvokePackageDone),
}

/// Runtime -> Module: start execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Invoke {
    #[serde(default = "Invoke::default_version")]
    pub version: u32,
    pub command: String,
    pub args: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub input: bool,
}

impl Invoke {
    fn default_version() -> u32 {
        1
    }

    /// Build an INVOKE from string-valued args (the common case from CLI flags).
    pub fn from_str_args(
        command: impl Into<String>,
        args: &BTreeMap<String, String>,
        input: bool,
    ) -> Self {
        Invoke {
            version: 1,
            command: command.into(),
            args: args
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect(),
            input,
        }
    }
}

/// Module -> Runtime: result record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Output {
    pub record: serde_json::Value,
}

/// Module -> Runtime: execution complete.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Done {
    pub exit_code: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Runtime -> Module: streaming input data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Input {
    pub data: String,
    #[serde(default)]
    pub eof: bool,
}

/// Module -> Runtime: progress report (fire-and-forget).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Progress {
    pub fraction: f64,
}

/// Module -> Runtime: invoke another installed package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvokePackage {
    pub id: u64,
    pub package: String,
    pub command: String,
    pub args: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub input: bool,
}

/// Runtime -> Module: one streamed record from an invoked package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvokePackageOutput {
    pub id: u64,
    pub record: serde_json::Value,
}

/// Runtime -> Module: nested package invocation is complete.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvokePackageDone {
    pub id: u64,
    pub exit_code: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Module -> Runtime: capability request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityReq {
    pub id: u64,
    pub domain: String,
    pub op: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Runtime -> Module: capability response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityRes {
    pub id: u64,
    #[serde(flatten)]
    pub result: CapabilityResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum CapabilityResult {
    Ok { data: serde_json::Value },
    Error { error: CapabilityError },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityError {
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DaemonRefusal {
    VersionMismatch { daemon: String, client: String },
    PackageNotFound { name: String },
    WasmArtifactMissing { path: String },
    LoadFailed { reason: String },
    InvalidRequest { reason: String },
    Other { message: String },
}

/// Normalize an OS path to forward-slash form for cross-platform transport.
pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// Resolve a (potentially relative) path against a CWD, returning the
/// normalized forward-slash form.
pub fn resolve_path(path: &str, cwd: &str) -> String {
    let normalized = normalize_path(path);
    if std::path::Path::new(&normalized).is_absolute() || normalized.starts_with('/') {
        return normalized;
    }
    let base = normalize_path(cwd);
    let base = base.trim_end_matches('/');
    format!("{}/{}", base, normalized)
}

impl CapabilityError {
    pub fn from_io(err: &std::io::Error) -> Self {
        CapabilityError {
            kind: io_error_kind_to_str(err.kind()),
            message: err.to_string(),
        }
    }

    pub fn to_io(&self) -> std::io::Error {
        std::io::Error::new(str_to_io_error_kind(&self.kind), &*self.message)
    }
}

fn io_error_kind_to_str(kind: std::io::ErrorKind) -> String {
    match kind {
        std::io::ErrorKind::NotFound => "not_found",
        std::io::ErrorKind::PermissionDenied => "permission_denied",
        std::io::ErrorKind::ConnectionRefused => "connection_refused",
        std::io::ErrorKind::ConnectionReset => "connection_reset",
        std::io::ErrorKind::ConnectionAborted => "connection_aborted",
        std::io::ErrorKind::AlreadyExists => "already_exists",
        std::io::ErrorKind::InvalidInput => "invalid_input",
        std::io::ErrorKind::InvalidData => "invalid_data",
        std::io::ErrorKind::TimedOut => "timed_out",
        std::io::ErrorKind::Interrupted => "interrupted",
        std::io::ErrorKind::UnexpectedEof => "unexpected_eof",
        _ => "other",
    }
    .to_string()
}

fn str_to_io_error_kind(s: &str) -> std::io::ErrorKind {
    match s {
        "not_found" => std::io::ErrorKind::NotFound,
        "permission_denied" => std::io::ErrorKind::PermissionDenied,
        "connection_refused" => std::io::ErrorKind::ConnectionRefused,
        "connection_reset" => std::io::ErrorKind::ConnectionReset,
        "connection_aborted" => std::io::ErrorKind::ConnectionAborted,
        "already_exists" => std::io::ErrorKind::AlreadyExists,
        "invalid_input" => std::io::ErrorKind::InvalidInput,
        "invalid_data" => std::io::ErrorKind::InvalidData,
        "timed_out" => std::io::ErrorKind::TimedOut,
        "interrupted" => std::io::ErrorKind::Interrupted,
        "unexpected_eof" => std::io::ErrorKind::UnexpectedEof,
        _ => std::io::ErrorKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn base64_roundtrip() {
        let encoded = base64_encode(b"hello world");
        assert_eq!(base64_decode(&encoded).unwrap(), b"hello world");
    }

    #[test]
    fn invoke_roundtrip() {
        let msg = Message::Invoke(Invoke {
            version: 1,
            command: "default".into(),
            args: BTreeMap::from([
                ("path".into(), json!("/tmp")),
                ("all".into(), json!("true")),
            ]),
            input: false,
        });
        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json_str).unwrap();
        assert_eq!(msg, parsed);
        assert!(json_str.contains("\"type\":\"invoke\""));
        assert!(json_str.contains("\"version\":1"));
        assert!(!json_str.contains("\"input\""));
    }

    #[test]
    fn invoke_without_version_deserializes() {
        let json_str = r#"{"type":"invoke","command":"default","args":{}}"#;
        let parsed: Message = serde_json::from_str(json_str).unwrap();
        match parsed {
            Message::Invoke(inv) => assert_eq!(inv.version, 1),
            _ => panic!("expected Invoke"),
        }
    }

    #[test]
    fn capability_error_maps_io_roundtrip() {
        let original = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let cap_err = CapabilityError::from_io(&original);
        assert_eq!(cap_err.kind, "not_found");
        assert_eq!(cap_err.to_io().kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn daemon_refusal_roundtrip() {
        let refusal = DaemonRefusal::VersionMismatch {
            daemon: "1.2.0".into(),
            client: "1.1.0".into(),
        };
        let json = serde_json::to_string(&refusal).unwrap();
        assert!(json.contains("\"kind\":\"version_mismatch\""));
        let parsed: DaemonRefusal = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, refusal);
    }

    #[test]
    fn invoke_package_roundtrip() {
        let msg = Message::InvokePackage(InvokePackage {
            id: 9,
            package: "cat".into(),
            command: "default".into(),
            args: BTreeMap::from([("file".into(), "notes.txt".into())]),
            input: false,
        });
        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, msg);
        assert!(json_str.contains("\"type\":\"invoke_package\""));
    }
}
