use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandInvocationRequest {
    pub package: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
    pub context: InvocationContext,
    /// Run to completion even if the client disconnects mid-stream; errors
    /// then sink to the daemon log instead of the (gone) client. Additive and
    /// default-false: requests without the field keep cancel-on-disconnect
    /// semantics.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub detach: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvocationContext {
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InvocationEvent {
    Output {
        record: serde_json::Value,
    },
    Progress {
        fraction: f64,
    },
    Message {
        level: InvocationMessageLevel,
        text: String,
    },
    Done {
        exit_code: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InvocationMessageLevel {
    Info,
    Warning,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_invocation_request_roundtrips() {
        let request = CommandInvocationRequest {
            package: "echo".into(),
            command: "default".into(),
            args: BTreeMap::from([("text".into(), "hello".into())]),
            context: InvocationContext {
                cwd: "/workspace".into(),
            },
            detach: false,
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: CommandInvocationRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed, request);
        assert!(
            !json.contains("detach"),
            "default detach stays off the wire"
        );
    }

    #[test]
    fn command_invocation_request_detach_defaults_false_and_roundtrips() {
        let parsed: CommandInvocationRequest =
            serde_json::from_str(r#"{"package":"echo","command":"default","context":{"cwd":"."}}"#)
                .unwrap();
        assert!(!parsed.detach);

        let request = CommandInvocationRequest {
            package: "watch".into(),
            command: "default".into(),
            args: BTreeMap::new(),
            context: InvocationContext { cwd: ".".into() },
            detach: true,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"detach\":true"));
        let reparsed: CommandInvocationRequest = serde_json::from_str(&json).unwrap();
        assert!(reparsed.detach);
    }

    #[test]
    fn invocation_events_roundtrip() {
        let events = vec![
            InvocationEvent::Output {
                record: serde_json::json!({"value": "hello"}),
            },
            InvocationEvent::Progress { fraction: 0.5 },
            InvocationEvent::Message {
                level: InvocationMessageLevel::Warning,
                text: "careful".into(),
            },
            InvocationEvent::Done {
                exit_code: 1,
                error: Some("failed".into()),
            },
        ];

        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let parsed: InvocationEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, event);
        }
    }
}
