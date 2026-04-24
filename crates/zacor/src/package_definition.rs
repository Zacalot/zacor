use crate::error::*;
use crate::platform;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageDefinition {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wasm: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub protocol: bool,
    pub commands: BTreeMap<String, CommandDefinition>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config: BTreeMap<String, serde_yml::Value>,
    #[serde(default, skip_serializing_if = "DependsSection::is_empty")]
    pub depends: DependsSection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<ServiceSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<ExecutionSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildSection>,
    #[serde(
        rename = "project-data",
        default,
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub project_data: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandDefinition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, ArgumentDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invoke: Option<InvokeTemplate>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub commands: BTreeMap<String, CommandDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputType>,
    #[serde(rename = "inline-input-fallback", default, skip_serializing_if = "Option::is_none")]
    pub inline_input_fallback: Option<InlineInputFallback>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<OutputDeclaration>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum InlineInputFallback {
    StringValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputDeclaration {
    /// Legacy field — old `type: text|table|record` format.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub output_type: Option<OutputType>,
    /// New field — how many results (one or many).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cardinality: Option<Cardinality>,
    /// New field — CLI rendering mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<DisplayType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<BTreeMap<String, String>>,
}

impl OutputDeclaration {
    /// Resolve cardinality from new or legacy fields.
    pub fn resolved_cardinality(&self) -> Cardinality {
        if let Some(c) = self.cardinality {
            return c;
        }
        match self.output_type {
            Some(OutputType::Table) => Cardinality::Many,
            _ => Cardinality::One,
        }
    }

    /// Resolve display type from new or legacy fields.
    pub fn resolved_display(&self) -> Option<DisplayType> {
        if let Some(d) = self.display {
            return Some(d);
        }
        self.output_type.map(|t| match t {
            OutputType::Text => DisplayType::Text,
            OutputType::Table => DisplayType::Table,
            OutputType::Record => DisplayType::Record,
        })
    }

    /// Backward compat — derive OutputType from new fields.
    pub fn resolved_output_type(&self) -> OutputType {
        if let Some(t) = self.output_type {
            return t;
        }
        match self.resolved_display() {
            Some(DisplayType::Text) => OutputType::Text,
            Some(DisplayType::Table) => OutputType::Table,
            Some(DisplayType::Record) => OutputType::Record,
            None => match self.resolved_cardinality() {
                Cardinality::One => OutputType::Record,
                Cardinality::Many => OutputType::Table,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputType {
    Text,
    Table,
    Record,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Cardinality {
    One,
    Many,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DisplayType {
    Text,
    Table,
    Record,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InputType {
    Text,
    Jsonl,
    Binary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InvokeTemplate {
    String(String),
    Array(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgumentDefinition {
    #[serde(rename = "type")]
    pub arg_type: ArgType,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_yml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rest: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ArgType {
    String,
    Number,
    Integer,
    Bool,
    Path,
    Choice,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependsSection {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<PackageDep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binaries: Vec<BinaryDep>,
}

impl DependsSection {
    pub fn is_empty(&self) -> bool {
        self.packages.is_empty() && self.binaries.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageDep {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryDep {
    pub binary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub library: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// Parse a package.yaml from a file path.
pub fn parse_file(path: &Path) -> Result<PackageDefinition> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read package definition at {}", path.display()))?;
    parse(&contents)
}

/// Parse a package.yaml from a YAML string.
pub fn parse(yaml: &str) -> Result<PackageDefinition> {
    let def: PackageDefinition =
        serde_yml::from_str(yaml).context("failed to parse package.yaml")?;
    validate(&def)?;
    Ok(def)
}

fn validate(def: &PackageDefinition) -> Result<()> {
    platform::validate_package_name(&def.name).context("package.yaml: invalid package name")?;

    if def.version.is_empty() {
        bail!("package.yaml: version is required");
    }

    let artifact_count = [&def.run, &def.binary, &def.wasm]
        .iter()
        .filter(|o| o.is_some())
        .count();
    if artifact_count > 1 {
        bail!("package.yaml: 'run', 'binary', and 'wasm' are mutually exclusive");
    }

    if def.commands.is_empty() {
        bail!("package.yaml: at least one command is required");
    }

    for key in def.config.keys() {
        platform::validate_config_key(key)
            .with_context(|| format!("package.yaml: invalid config key '{}'", key))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_binary_package() {
        let yaml = r#"
name: ripgrep
version: "14.1.0"
binary: rg
description: Fast line-oriented search tool
commands:
  default:
    description: Search for a pattern
    args:
      pattern:
        type: string
        required: true
      path:
        type: path
"#;
        let def = parse(yaml).unwrap();
        assert_eq!(def.name, "ripgrep");
        assert_eq!(def.version, "14.1.0");
        assert_eq!(def.binary.as_deref(), Some("rg"));
        assert_eq!(
            def.description.as_deref(),
            Some("Fast line-oriented search tool")
        );
        assert!(def.commands.contains_key("default"));
        let cmd = &def.commands["default"];
        assert!(cmd.args["pattern"].required);
        assert!(!cmd.args["path"].required);
        assert_eq!(cmd.args["pattern"].arg_type, ArgType::String);
        assert_eq!(cmd.args["path"].arg_type, ArgType::Path);
    }

    #[test]
    fn test_definition_only_string_invoke() {
        let yaml = r#"
name: ffmpeg-convert
version: "1.0.0"
commands:
  convert:
    description: Convert media files
    args:
      input:
        type: path
        required: true
      format:
        type: choice
        values: [mp3, wav, flac]
        default: mp3
      output:
        type: path
        required: true
    invoke: "ffmpeg -i {input} -f {format} {output}"
"#;
        let def = parse(yaml).unwrap();
        assert!(def.binary.is_none());
        let cmd = &def.commands["convert"];
        match cmd.invoke.as_ref().unwrap() {
            InvokeTemplate::String(s) => assert!(s.contains("ffmpeg")),
            _ => panic!("expected string invoke"),
        }
    }

    #[test]
    fn test_definition_only_array_invoke() {
        let yaml = r#"
name: ffmpeg-convert
version: "1.0.0"
commands:
  convert:
    args:
      input:
        type: path
        required: true
      output:
        type: path
        required: true
    invoke:
      - ffmpeg
      - "-i"
      - "{input}"
      - "{output}"
"#;
        let def = parse(yaml).unwrap();
        let cmd = &def.commands["convert"];
        match cmd.invoke.as_ref().unwrap() {
            InvokeTemplate::Array(arr) => {
                assert_eq!(arr[0], "ffmpeg");
                assert_eq!(arr.len(), 4);
            }
            _ => panic!("expected array invoke"),
        }
    }

    #[test]
    fn test_missing_name() {
        let yaml = r#"
version: "1.0.0"
commands:
  default:
    description: test
"#;
        assert!(parse(yaml).is_err());
    }

    #[test]
    fn test_missing_version() {
        let yaml = r#"
name: test
commands:
  default:
    description: test
"#;
        assert!(parse(yaml).is_err());
    }

    #[test]
    fn test_missing_commands() {
        let yaml = r#"
name: test
version: "1.0.0"
"#;
        assert!(parse(yaml).is_err());
    }

    #[test]
    fn test_empty_commands() {
        let yaml = r#"
name: test
version: "1.0.0"
commands: {}
"#;
        let err = parse(yaml).unwrap_err().to_string();
        assert!(err.contains("at least one command"), "got: {}", err);
    }

    #[test]
    fn test_config_section() {
        let yaml = r#"
name: my-pkg
version: "1.0.0"
binary: my-pkg
config:
  model: base
  language: auto
commands:
  default:
    description: Transcribe audio
"#;
        let def = parse(yaml).unwrap();
        assert_eq!(def.config.len(), 2);
        assert!(def.config.contains_key("model"));
        assert!(def.config.contains_key("language"));
    }

    #[test]
    fn test_invalid_config_key() {
        let yaml = r#"
name: my-pkg
version: "1.0.0"
binary: my-pkg
config:
  output_format: json
commands:
  default:
    description: test
"#;
        let err = parse(yaml).unwrap_err().to_string();
        assert!(err.contains("invalid"), "got: {}", err);
    }

    #[test]
    fn test_nested_commands() {
        let yaml = r#"
name: my-pkg
version: "1.0.0"
binary: my-pkg
commands:
  transcribe:
    description: Transcribe audio
    commands:
      batch:
        description: Batch transcribe
        args:
          files:
            type: string
            required: true
  translate:
    description: Translate audio
"#;
        let def = parse(yaml).unwrap();
        assert!(def.commands.contains_key("transcribe"));
        let transcribe = &def.commands["transcribe"];
        assert!(transcribe.commands.contains_key("batch"));
    }

    #[test]
    fn test_depends_section() {
        let yaml = r#"
name: my-tool
version: "1.0.0"
commands:
  default:
    description: Run tool
depends:
  packages:
    - name: my-pkg
    - name: other-tool
      version: ">=1.0"
      source: github.com/user/other-tool
  binaries:
    - binary: ffmpeg
      check: "ffmpeg -version"
      install_hint: "Install ffmpeg via your package manager"
"#;
        let def = parse(yaml).unwrap();
        assert_eq!(def.depends.packages.len(), 2);
        assert_eq!(def.depends.packages[0].name, "my-pkg");
        assert!(def.depends.packages[0].source.is_none());
        assert_eq!(
            def.depends.packages[1].source.as_deref(),
            Some("github.com/user/other-tool")
        );
        assert_eq!(def.depends.binaries.len(), 1);
        assert_eq!(def.depends.binaries[0].binary, "ffmpeg");
    }

    #[test]
    fn test_unknown_fields_ignored() {
        let yaml = r#"
name: test
version: "1.0.0"
future_field: some_value
another_unknown: 42
commands:
  default:
    description: test
    unknown_cmd_field: true
"#;
        let def = parse(yaml).unwrap();
        assert_eq!(def.name, "test");
    }

    #[test]
    fn test_description_absent() {
        let yaml = r#"
name: test
version: "1.0.0"
binary: test
commands:
  default:
    args:
      input:
        type: string
"#;
        let def = parse(yaml).unwrap();
        assert!(def.description.is_none());
    }

    #[test]
    fn test_service_section() {
        let yaml = r#"
name: my-server
version: "1.0.0"
binary: my-server
service:
  start: "my-server --port {port}"
  port: 8080
  health: /health
  startup: eager
execution:
  default: service
commands:
  default:
    description: Run server
"#;
        let def = parse(yaml).unwrap();
        let svc = def.service.as_ref().unwrap();
        assert_eq!(svc.start.as_deref(), Some("my-server --port {port}"));
        assert_eq!(svc.port, Some(8080));
        assert_eq!(svc.health.as_deref(), Some("/health"));
        assert_eq!(svc.startup.as_deref(), Some("eager"));
        let exec = def.execution.as_ref().unwrap();
        assert_eq!(exec.default.as_deref(), Some("service"));
    }

    #[test]
    fn test_library_service_section() {
        let yaml = r#"
name: zr-lib
version: "0.1.0"
wasm: zr-lib.wasm
protocol: true
service:
  library: true
  idle_timeout_secs: 600
  max_concurrent: 4
commands:
  default:
    description: Library entrypoint
"#;
        let def = parse(yaml).unwrap();
        let svc = def.service.as_ref().unwrap();
        assert_eq!(svc.start, None);
        assert!(svc.library);
        assert_eq!(svc.idle_timeout_secs, Some(600));
        assert_eq!(svc.max_concurrent, Some(4));
    }

    #[test]
    fn test_build_section() {
        let yaml = r#"
name: echo
version: "0.2.0"
binary: echo
build:
  command: "cargo build --release --bin echo"
  output: target/release
commands:
  default:
    description: Echo text
"#;
        let def = parse(yaml).unwrap();
        let build = def.build.as_ref().unwrap();
        assert_eq!(
            build.command.as_deref(),
            Some("cargo build --release --bin echo")
        );
        assert_eq!(build.output.as_deref(), Some("target/release"));
    }

    #[test]
    fn test_output_declaration_text() {
        let yaml = r#"
name: echo
version: "0.2.0"
binary: echo
commands:
  default:
    description: Echo text
    output:
      type: text
      field: text
      schema:
        text: string
"#;
        let def = parse(yaml).unwrap();
        let cmd = &def.commands["default"];
        let output = cmd.output.as_ref().unwrap();
        assert_eq!(output.resolved_output_type(), OutputType::Text);
        assert_eq!(output.field.as_deref(), Some("text"));
        assert!(!output.stream);
        let schema = output.schema.as_ref().unwrap();
        assert_eq!(schema["text"], "string");
    }

    #[test]
    fn test_output_declaration_table() {
        let yaml = r#"
name: ls
version: "0.2.0"
binary: ls
commands:
  default:
    description: List entries
    output:
      type: table
      schema:
        name: string
        size: filesize
        kind: string
"#;
        let def = parse(yaml).unwrap();
        let cmd = &def.commands["default"];
        let output = cmd.output.as_ref().unwrap();
        assert_eq!(output.resolved_output_type(), OutputType::Table);
        assert!(output.field.is_none());
        assert!(!output.stream);
        let schema = output.schema.as_ref().unwrap();
        assert_eq!(schema.len(), 3);
        assert_eq!(schema["size"], "filesize");
    }

    #[test]
    fn test_output_declaration_streaming_table() {
        let yaml = r#"
name: cat
version: "0.2.0"
binary: cat
commands:
  default:
    description: Cat file
    output:
      type: table
      stream: true
      schema:
        line: number
        content: string
"#;
        let def = parse(yaml).unwrap();
        let cmd = &def.commands["default"];
        let output = cmd.output.as_ref().unwrap();
        assert_eq!(output.resolved_output_type(), OutputType::Table);
        assert!(output.stream);
    }

    #[test]
    fn test_invalid_output_type_rejected() {
        let yaml = r#"
name: bad
version: "1.0.0"
commands:
  default:
    output:
      type: invalid
"#;
        assert!(parse(yaml).is_err());
    }

    #[test]
    fn test_inline_input_fallback_parses() {
        let yaml = r#"
name: str
version: "0.1.0"
commands:
  capitalize:
    input: jsonl
    inline-input-fallback: string-value
"#;
        let def = parse(yaml).unwrap();
        let cmd = &def.commands["capitalize"];
        assert_eq!(
            cmd.inline_input_fallback,
            Some(InlineInputFallback::StringValue)
        );
    }

    #[test]
    fn test_no_output_section_backwards_compatible() {
        let yaml = r#"
name: test
version: "1.0.0"
commands:
  default:
    description: test
"#;
        let def = parse(yaml).unwrap();
        let cmd = &def.commands["default"];
        assert!(cmd.output.is_none());
    }

    #[test]
    fn test_project_data_true() {
        let yaml = r#"
name: bp
version: "1.0.0"
project-data: true
commands:
  default:
    description: Blueprint
"#;
        let def = parse(yaml).unwrap();
        assert!(def.project_data);
    }

    #[test]
    fn test_project_data_defaults_false() {
        let yaml = r#"
name: echo
version: "0.2.0"
commands:
  default:
    description: Echo text
"#;
        let def = parse(yaml).unwrap();
        assert!(!def.project_data);
    }

    #[test]
    fn test_existing_packages_still_parse_without_project_data() {
        let yaml = r#"
name: ripgrep
version: "14.1.0"
binary: rg
description: Fast search
commands:
  default:
    description: Search
    args:
      pattern:
        type: string
        required: true
"#;
        let def = parse(yaml).unwrap();
        assert!(!def.project_data);
        assert_eq!(def.name, "ripgrep");
    }

    #[test]
    fn test_valid_run_field() {
        let yaml = r#"
name: my-script
version: "1.0.0"
run: "python3 main.py"
commands:
  default:
    description: Run script
"#;
        let def = parse(yaml).unwrap();
        assert_eq!(def.run.as_deref(), Some("python3 main.py"));
        assert!(def.binary.is_none());
    }

    #[test]
    fn test_run_and_binary_mutually_exclusive() {
        let yaml = r#"
name: bad
version: "1.0.0"
run: "python3 main.py"
binary: bad
commands:
  default:
    description: test
"#;
        let err = parse(yaml).unwrap_err().to_string();
        assert!(err.contains("mutually exclusive"), "got: {}", err);
    }

    #[test]
    fn test_neither_run_nor_binary_allowed() {
        let yaml = r#"
name: invoke-only
version: "1.0.0"
commands:
  default:
    description: test
    invoke: "echo hello"
"#;
        let def = parse(yaml).unwrap();
        assert!(def.run.is_none());
        assert!(def.binary.is_none());
    }

    #[test]
    fn test_no_build_section_backwards_compatible() {
        let yaml = r#"
name: test
version: "1.0.0"
commands:
  default:
    description: test
"#;
        let def = parse(yaml).unwrap();
        assert!(def.build.is_none());
    }
}
