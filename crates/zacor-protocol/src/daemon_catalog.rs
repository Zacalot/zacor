use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledPackageSummary {
    pub name: String,
    pub version: String,
    pub active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageDescriptor {
    pub name: String,
    pub version: String,
    pub active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub commands: BTreeMap<String, CommandDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandDescriptor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, ArgumentDescriptor>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub commands: BTreeMap<String, CommandDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<OutputDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArgumentDescriptor {
    pub arg_type: ArgumentType,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rest: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputDescriptor {
    pub cardinality: OutputCardinality,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<OutputDisplay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stream: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ArgumentType {
    String,
    Number,
    Integer,
    Bool,
    Path,
    Choice,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InputKind {
    Text,
    Jsonl,
    Binary,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputCardinality {
    One,
    Many,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputDisplay {
    Text,
    Table,
    Record,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_package_summary_roundtrip() {
        let summary = InstalledPackageSummary {
            name: "echo".into(),
            version: "0.2.0".into(),
            active: true,
            description: Some("Echo text".into()),
        };

        let json = serde_json::to_string(&summary).unwrap();
        let parsed: InstalledPackageSummary = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed, summary);
    }

    #[test]
    fn package_descriptor_roundtrip_preserves_nested_commands() {
        let descriptor = PackageDescriptor {
            name: "tool".into(),
            version: "1.0.0".into(),
            active: false,
            description: Some("Test tool".into()),
            commands: BTreeMap::from([(
                "default".into(),
                CommandDescriptor {
                    description: Some("Top-level command".into()),
                    args: BTreeMap::from([(
                        "path".into(),
                        ArgumentDescriptor {
                            arg_type: ArgumentType::Path,
                            required: true,
                            flag: None,
                            values: None,
                            rest: false,
                        },
                    )]),
                    commands: BTreeMap::from([(
                        "batch".into(),
                        CommandDescriptor {
                            description: Some("Nested command".into()),
                            args: BTreeMap::new(),
                            commands: BTreeMap::new(),
                            input: Some(InputKind::Text),
                            output: Some(OutputDescriptor {
                                cardinality: OutputCardinality::Many,
                                display: Some(OutputDisplay::Table),
                                field: Some("value".into()),
                                stream: true,
                                schema: Some(BTreeMap::from([("value".into(), "string".into())])),
                            }),
                        },
                    )]),
                    input: None,
                    output: None,
                },
            )]),
        };

        let json = serde_json::to_string(&descriptor).unwrap();
        let parsed: PackageDescriptor = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed, descriptor);
    }
}
