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

// ─── Shared Base64 Helpers ───────────────────────────────────────────

/// Encode bytes to base64 string.
pub fn base64_encode(data: &[u8]) -> String {
    data_encoding::BASE64.encode(data)
}

/// Decode base64 string to bytes.
pub fn base64_decode(s: &str) -> Result<Vec<u8>, std::io::Error> {
    data_encoding::BASE64
        .decode(s.as_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

// ─── Protocol Message Types ──────────────────────────────────────────

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

// ─── Path Normalization ──────────────────────────────────────────────

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

// ─── CapabilityError <-> std::io::Error Mapping ──────────────────────

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

    // ─── New Protocol Message Roundtrip Tests ────────────────────────

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
        // input: false is skipped
        assert!(!json_str.contains("\"input\""));
    }

    #[test]
    fn invoke_without_version_deserializes() {
        // Backwards compat: old INVOKE without version field defaults to 1
        let json_str = r#"{"type":"invoke","command":"default","args":{}}"#;
        let parsed: Message = serde_json::from_str(json_str).unwrap();
        match parsed {
            Message::Invoke(inv) => assert_eq!(inv.version, 1),
            _ => panic!("expected Invoke"),
        }
    }

    #[test]
    fn invoke_with_input_roundtrip() {
        let msg = Message::Invoke(Invoke {
            version: 1,
            command: "default".into(),
            args: BTreeMap::new(),
            input: true,
        });
        let json_str = serde_json::to_string(&msg).unwrap();
        assert!(json_str.contains("\"input\":true"));
        let parsed: Message = serde_json::from_str(&json_str).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn output_roundtrip() {
        let msg = Message::Output(Output {
            record: json!({"name": "file.txt", "size": 1024}),
        });
        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json_str).unwrap();
        assert_eq!(msg, parsed);
        assert!(json_str.contains("\"type\":\"output\""));
    }

    #[test]
    fn done_success_roundtrip() {
        let msg = Message::Done(Done {
            exit_code: 0,
            error: None,
        });
        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json_str).unwrap();
        assert_eq!(msg, parsed);
        assert!(!json_str.contains("\"error\""));
    }

    #[test]
    fn done_error_roundtrip() {
        let msg = Message::Done(Done {
            exit_code: 1,
            error: Some("file not found: data.txt".into()),
        });
        let json_str = serde_json::to_string(&msg).unwrap();
        assert!(json_str.contains("\"error\":\"file not found: data.txt\""));
        let parsed: Message = serde_json::from_str(&json_str).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn done_without_error_field_deserializes() {
        let json_str = r#"{"type":"done","exit_code":0}"#;
        let parsed: Message = serde_json::from_str(json_str).unwrap();
        assert_eq!(
            parsed,
            Message::Done(Done {
                exit_code: 0,
                error: None
            })
        );
    }

    #[test]
    fn input_roundtrip() {
        let msg = Message::Input(Input {
            data: "line of text\n".into(),
            eof: false,
        });
        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json_str).unwrap();
        assert_eq!(msg, parsed);
        assert!(json_str.contains("\"type\":\"input\""));
    }

    #[test]
    fn input_eof_roundtrip() {
        let msg = Message::Input(Input {
            data: String::new(),
            eof: true,
        });
        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json_str).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn progress_roundtrip() {
        let msg = Message::Progress(Progress { fraction: 0.75 });
        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json_str).unwrap();
        assert_eq!(msg, parsed);
        assert!(json_str.contains("\"type\":\"progress\""));
        assert!(json_str.contains("0.75"));
    }

    #[test]
    fn capability_req_roundtrip() {
        let msg = Message::CapabilityReq(CapabilityReq {
            id: 1,
            domain: "fs".into(),
            op: "read_string".into(),
            params: json!({"path": "config.yaml"}),
        });
        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json_str).unwrap();
        assert_eq!(msg, parsed);
        assert!(json_str.contains("\"type\":\"capability_req\""));
    }

    #[test]
    fn capability_res_ok_roundtrip() {
        let msg = Message::CapabilityRes(CapabilityRes {
            id: 1,
            result: CapabilityResult::Ok {
                data: json!({"content": "file contents"}),
            },
        });
        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json_str).unwrap();
        assert_eq!(msg, parsed);
        assert!(json_str.contains("\"status\":\"ok\""));
    }

    #[test]
    fn capability_res_error_roundtrip() {
        let msg = Message::CapabilityRes(CapabilityRes {
            id: 1,
            result: CapabilityResult::Error {
                error: CapabilityError {
                    kind: "not_found".into(),
                    message: "file not found".into(),
                },
            },
        });
        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json_str).unwrap();
        assert_eq!(msg, parsed);
        assert!(json_str.contains("\"status\":\"error\""));
    }

    #[test]
    fn capability_req_res_id_matching() {
        let req = CapabilityReq {
            id: 42,
            domain: "fs".into(),
            op: "read_string".into(),
            params: json!({"path": "file.txt"}),
        };
        let res = CapabilityRes {
            id: 42,
            result: CapabilityResult::Ok {
                data: json!({"content": "hello"}),
            },
        };
        assert_eq!(req.id, res.id);
    }

    #[test]
    fn unknown_type_fails_deserialization() {
        // Unknown types cause deserialization errors — callers catch and skip,
        // implementing the "ignore unknown types" spec requirement.
        let json_str = r#"{"type":"unknown_future_type","data":123}"#;
        assert!(serde_json::from_str::<Message>(json_str).is_err());
    }

    // ─── Shared Utility Tests ────────────────────────────────────────

    #[test]
    fn normalize_path_forward_slashes() {
        assert_eq!(normalize_path("src/main.rs"), "src/main.rs");
    }

    #[test]
    fn normalize_path_backslashes() {
        assert_eq!(normalize_path("src\\main.rs"), "src/main.rs");
    }

    #[test]
    fn normalize_path_mixed() {
        assert_eq!(
            normalize_path("C:\\Users\\xyz/projects"),
            "C:/Users/xyz/projects"
        );
    }

    #[test]
    fn resolve_path_absolute() {
        assert_eq!(
            resolve_path("/home/user/file.txt", "/other"),
            "/home/user/file.txt"
        );
    }

    #[test]
    fn resolve_path_relative() {
        assert_eq!(
            resolve_path("src/main.rs", "/home/user/project"),
            "/home/user/project/src/main.rs"
        );
    }

    #[test]
    fn resolve_path_windows_backslash() {
        assert_eq!(
            resolve_path("src\\main.rs", "C:\\Users\\xyz\\project"),
            "C:/Users/xyz/project/src/main.rs"
        );
    }

    #[test]
    fn capability_error_from_io_roundtrip() {
        let original = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let cap_err = CapabilityError::from_io(&original);
        assert_eq!(cap_err.kind, "not_found");
        let converted = cap_err.to_io();
        assert_eq!(converted.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn capability_error_all_io_kinds() {
        let kinds = [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::ConnectionRefused,
            std::io::ErrorKind::ConnectionReset,
            std::io::ErrorKind::ConnectionAborted,
            std::io::ErrorKind::AlreadyExists,
            std::io::ErrorKind::InvalidInput,
            std::io::ErrorKind::InvalidData,
            std::io::ErrorKind::TimedOut,
            std::io::ErrorKind::Interrupted,
            std::io::ErrorKind::UnexpectedEof,
        ];
        for kind in kinds {
            let err = std::io::Error::new(kind, "test");
            let cap_err = CapabilityError::from_io(&err);
            let back = cap_err.to_io();
            assert_eq!(back.kind(), kind);
        }
    }
}
