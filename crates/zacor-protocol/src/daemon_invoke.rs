use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandInvocationRequest {
    pub package: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
    pub context: InvocationContext,
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
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: CommandInvocationRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed, request);
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
