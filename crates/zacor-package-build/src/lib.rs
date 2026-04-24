use std::collections::BTreeMap;
use std::path::Path;

/// Find the workspace root by walking up from CARGO_MANIFEST_DIR looking for
/// a Cargo.toml that contains `[workspace]`. Returns the relative path from
/// the crate to the workspace target/release dir, or None if not a workspace member.
fn workspace_target_rel_path() -> Option<String> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let manifest_path = Path::new(&manifest_dir);
    let mut dir = manifest_path.parent()?;
    loop {
        let candidate = dir.join("Cargo.toml");
        if let Ok(content) = std::fs::read_to_string(&candidate) {
            if content.contains("[workspace]") {
                let rel = pathdiff::diff_paths(dir, manifest_path)?;
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                return Some(format!("{rel_str}/target/release"));
            }
        }
        dir = dir.parent()?;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    String,
    Number,
    Integer,
    Bool,
    Path,
    Choice,
    Filesize,
    Datetime,
    Duration,
    Url,
}

impl ValueKind {
    pub fn as_yaml(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Integer => "integer",
            Self::Bool => "bool",
            Self::Path => "path",
            Self::Choice => "choice",
            Self::Filesize => "filesize",
            Self::Datetime => "datetime",
            Self::Duration => "duration",
            Self::Url => "url",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultValue {
    String(&'static str),
    Number(i64),
    Bool(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub description: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArgSchemaInfo {
    pub name: &'static str,
    pub arg_type: ValueKind,
    pub required: bool,
    pub flag: Option<&'static str>,
    pub default: Option<DefaultValue>,
    pub rest: bool,
}

impl ArgSchemaInfo {
    pub fn string(name: &'static str) -> Self {
        Self::new(name, ValueKind::String)
    }

    pub fn number(name: &'static str) -> Self {
        Self::new(name, ValueKind::Number)
    }

    pub fn integer(name: &'static str) -> Self {
        Self::new(name, ValueKind::Integer)
    }

    pub fn bool(name: &'static str) -> Self {
        Self::new(name, ValueKind::Bool)
    }

    pub fn path(name: &'static str) -> Self {
        Self::new(name, ValueKind::Path)
    }

    pub fn choice(name: &'static str) -> Self {
        Self::new(name, ValueKind::Choice)
    }

    pub fn new(name: &'static str, arg_type: ValueKind) -> Self {
        Self {
            name,
            arg_type,
            required: false,
            flag: None,
            default: None,
            rest: false,
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    pub fn flag(mut self, flag: &'static str) -> Self {
        self.flag = Some(flag);
        self
    }

    pub fn default(mut self, default: DefaultValue) -> Self {
        self.default = Some(default);
        self
    }

    pub fn rest(mut self) -> Self {
        self.rest = true;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldSchemaInfo {
    pub name: &'static str,
    pub field_type: ValueKind,
}

impl FieldSchemaInfo {
    pub fn string(name: &'static str) -> Self {
        Self::new(name, ValueKind::String)
    }

    pub fn number(name: &'static str) -> Self {
        Self::new(name, ValueKind::Number)
    }

    pub fn integer(name: &'static str) -> Self {
        Self::new(name, ValueKind::Integer)
    }

    pub fn bool(name: &'static str) -> Self {
        Self::new(name, ValueKind::Bool)
    }

    pub fn path(name: &'static str) -> Self {
        Self::new(name, ValueKind::Path)
    }

    pub fn filesize(name: &'static str) -> Self {
        Self::new(name, ValueKind::Filesize)
    }

    pub fn datetime(name: &'static str) -> Self {
        Self::new(name, ValueKind::Datetime)
    }

    pub fn duration(name: &'static str) -> Self {
        Self::new(name, ValueKind::Duration)
    }

    pub fn url(name: &'static str) -> Self {
        Self::new(name, ValueKind::Url)
    }

    pub fn new(name: &'static str, field_type: ValueKind) -> Self {
        Self { name, field_type }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    Text,
    Record,
    Table,
}

impl OutputKind {
    fn as_yaml(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Record => "record",
            Self::Table => "table",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputSpec {
    pub kind: Option<OutputKind>,
    pub field: Option<&'static str>,
    pub stream: bool,
    pub schema: Vec<FieldSchemaInfo>,
}

impl OutputSpec {
    pub fn infer(schema: &[FieldSchemaInfo]) -> Self {
        Self {
            kind: None,
            field: None,
            stream: false,
            schema: schema.to_vec(),
        }
    }

    pub fn text(field: &'static str, schema: &[FieldSchemaInfo]) -> Self {
        Self {
            kind: Some(OutputKind::Text),
            field: Some(field),
            stream: false,
            schema: schema.to_vec(),
        }
    }

    pub fn record(schema: &[FieldSchemaInfo]) -> Self {
        Self {
            kind: Some(OutputKind::Record),
            field: None,
            stream: false,
            schema: schema.to_vec(),
        }
    }

    pub fn table(schema: &[FieldSchemaInfo]) -> Self {
        Self {
            kind: Some(OutputKind::Table),
            field: None,
            stream: false,
            schema: schema.to_vec(),
        }
    }

    pub fn streaming_table(schema: &[FieldSchemaInfo]) -> Self {
        Self {
            kind: Some(OutputKind::Table),
            field: None,
            stream: true,
            schema: schema.to_vec(),
        }
    }

    pub fn stream(mut self) -> Self {
        self.stream = true;
        self
    }

    pub fn kind(mut self, kind: OutputKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn field(mut self, field: &'static str) -> Self {
        self.field = Some(field);
        self
    }

    fn resolved_kind(&self) -> OutputKind {
        if let Some(kind) = self.kind {
            return kind;
        }

        if self.schema.len() == 1 && self.schema[0].field_type == ValueKind::String {
            OutputKind::Text
        } else {
            OutputKind::Record
        }
    }

    fn resolved_field(&self, kind: OutputKind) -> Option<&'static str> {
        if let Some(field) = self.field {
            return Some(field);
        }

        if kind == OutputKind::Text && self.schema.len() == 1 {
            Some(self.schema[0].name)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Text,
    Jsonl,
    Binary,
}

impl InputKind {
    fn as_yaml(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Jsonl => "jsonl",
            Self::Binary => "binary",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineInputFallbackKind {
    StringValue,
}

impl InlineInputFallbackKind {
    fn as_yaml(self) -> &'static str {
        match self {
            Self::StringValue => "string-value",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: Option<&'static str>,
    pub description: Option<&'static str>,
    pub args: Vec<ArgSchemaInfo>,
    pub input: Option<InputKind>,
    pub inline_input_fallback: Option<InlineInputFallbackKind>,
    pub output: Option<OutputSpec>,
    pub subcommands: Vec<CommandSpec>,
}

impl CommandSpec {
    pub fn named(name: &'static str) -> Self {
        Self {
            name: Some(name),
            description: None,
            args: Vec::new(),
            input: None,
            inline_input_fallback: None,
            output: None,
            subcommands: Vec::new(),
        }
    }

    pub fn implicit_default() -> Self {
        Self {
            name: None,
            description: None,
            args: Vec::new(),
            input: None,
            inline_input_fallback: None,
            output: None,
            subcommands: Vec::new(),
        }
    }

    pub fn description(mut self, description: &'static str) -> Self {
        self.description = Some(description);
        self
    }

    pub fn args(mut self, args: &[ArgSchemaInfo]) -> Self {
        self.args = args.to_vec();
        self
    }

    pub fn input(mut self, input: InputKind) -> Self {
        self.input = Some(input);
        self
    }

    pub fn inline_input_fallback(mut self, fallback: InlineInputFallbackKind) -> Self {
        self.inline_input_fallback = Some(fallback);
        self
    }

    pub fn output(mut self, output: OutputSpec) -> Self {
        self.output = Some(output);
        self
    }

    pub fn subcommand(mut self, sub: CommandSpec) -> Self {
        self.subcommands.push(sub);
        self
    }

    fn resolved_name(&self, command_count: usize) -> &'static str {
        match self.name {
            Some(name) => name,
            None if command_count == 1 => "default",
            None => panic!("multiple commands require explicit names"),
        }
    }
}

impl Default for CommandSpec {
    fn default() -> Self {
        Self {
            name: Some("default"),
            description: None,
            args: Vec::new(),
            input: None,
            inline_input_fallback: None,
            output: None,
            subcommands: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiResourceConfig {
    prompts_dir: &'static str,
    templates_dir: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceConfig {
    pub start: &'static str,
    pub port: u16,
    pub health: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageDependency {
    pub name: &'static str,
    pub version: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl PackageDependency {
    pub fn named(name: &'static str) -> Self {
        Self {
            name,
            version: None,
            source: None,
        }
    }

    pub fn version(mut self, version: &'static str) -> Self {
        self.version = Some(version);
        self
    }

    pub fn source(mut self, source: &'static str) -> Self {
        self.source = Some(source);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BinaryDependency {
    pub binary: &'static str,
    pub check: Option<&'static str>,
    pub install_hint: Option<&'static str>,
}

impl BinaryDependency {
    pub fn named(binary: &'static str) -> Self {
        Self {
            binary,
            check: None,
            install_hint: None,
        }
    }

    pub fn check(mut self, check: &'static str) -> Self {
        self.check = Some(check);
        self
    }

    pub fn install_hint(mut self, install_hint: &'static str) -> Self {
        self.install_hint = Some(install_hint);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSpec {
    pub info: PackageInfo,
    pub binary: Option<&'static str>,
    pub build_command: Option<&'static str>,
    pub build_output: Option<&'static str>,
    pub project_data: bool,
    pub commands: Vec<CommandSpec>,
    pub execution_default: Option<&'static str>,
    pub service: Option<ServiceConfig>,
    pub package_depends: Vec<PackageDependency>,
    pub binary_depends: Vec<BinaryDependency>,
    skills_config: Option<AiResourceConfig>,
    agents_config: Option<AiResourceConfig>,
}

impl PackageSpec {
    /// Create a PackageSpec reading version and description from Cargo env vars.
    ///
    /// The `name` is the zr package name (e.g. `"echo"`), not the crate name
    /// (e.g. `"zr-echo"`). Version comes from `CARGO_PKG_VERSION` and description
    /// from `CARGO_PKG_DESCRIPTION`.
    pub fn from_cargo(name: &'static str) -> Self {
        let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION not set");
        let version: &'static str = Box::leak(version.into_boxed_str());

        let description = std::env::var("CARGO_PKG_DESCRIPTION")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| &*Box::leak(s.into_boxed_str()) as &'static str);

        Self {
            info: PackageInfo {
                name,
                version,
                description,
            },
            binary: None,
            build_command: None,
            build_output: None,
            project_data: false,
            commands: Vec::new(),
            execution_default: None,
            service: None,
            package_depends: Vec::new(),
            binary_depends: Vec::new(),
            skills_config: None,
            agents_config: None,
        }
    }

    pub fn new(name: &'static str, version: &'static str) -> Self {
        Self {
            info: PackageInfo {
                name,
                version,
                description: None,
            },
            binary: None,
            build_command: None,
            build_output: None,
            project_data: false,
            commands: Vec::new(),
            execution_default: None,
            service: None,
            package_depends: Vec::new(),
            binary_depends: Vec::new(),
            skills_config: None,
            agents_config: None,
        }
    }

    pub fn description(mut self, description: &'static str) -> Self {
        self.info.description = Some(description);
        self
    }

    pub fn binary(mut self, binary: &'static str) -> Self {
        self.binary = Some(binary);
        self
    }

    pub fn build_command(mut self, build_command: &'static str) -> Self {
        self.build_command = Some(build_command);
        self
    }

    pub fn build_output(mut self, build_output: &'static str) -> Self {
        self.build_output = Some(build_output);
        self
    }

    pub fn project_data(mut self) -> Self {
        self.project_data = true;
        self
    }

    pub fn execution_default(mut self, mode: &'static str) -> Self {
        self.execution_default = Some(mode);
        self
    }

    pub fn service(mut self, start: &'static str, port: u16, health: &'static str) -> Self {
        self.service = Some(ServiceConfig {
            start,
            port,
            health,
        });
        self
    }

    pub fn depends_package(mut self, dependency: PackageDependency) -> Self {
        self.package_depends.push(dependency);
        self
    }

    pub fn depends_binary(mut self, dependency: BinaryDependency) -> Self {
        self.binary_depends.push(dependency);
        self
    }

    pub fn command(mut self, command: CommandSpec) -> Self {
        self.commands.push(command);
        self
    }

    /// Configure skill generation. Triggers build-time codegen and auto-injects
    /// `skill` and `template` commands into the generated package.yaml.
    pub fn skills(mut self, prompts_dir: &'static str, templates_dir: &'static str) -> Self {
        self.skills_config = Some(AiResourceConfig {
            prompts_dir,
            templates_dir,
        });
        self.commands.push(
            CommandSpec::named("skill")
                .description("Serve a resolved skill prompt")
                .args(&[
                    ArgSchemaInfo::string("name").required(),
                    ArgSchemaInfo::string("skill-args").rest(),
                ])
                .output(OutputSpec::text("body", &[FieldSchemaInfo::string("body")])),
        );
        self.commands.push(
            CommandSpec::named("template")
                .description("Serve a named reusable template")
                .args(&[
                    ArgSchemaInfo::string("name"),
                    ArgSchemaInfo::bool("list").flag("list"),
                ])
                .output(OutputSpec::text("body", &[FieldSchemaInfo::string("body")])),
        );
        self
    }

    /// Configure agent generation. Triggers build-time codegen.
    pub fn agents(mut self, prompts_dir: &'static str, templates_dir: &'static str) -> Self {
        self.agents_config = Some(AiResourceConfig {
            prompts_dir,
            templates_dir,
        });
        self
    }

    fn resolved_binary(&self) -> &'static str {
        self.binary.unwrap_or(self.info.name)
    }

    fn resolved_build_command(&self) -> String {
        match self.build_command {
            Some(command) => command.to_string(),
            None => format!("cargo build --release --bin {}", self.resolved_binary()),
        }
    }

    fn resolved_build_output(&self) -> String {
        self.build_output
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                workspace_target_rel_path().unwrap_or_else(|| "target/release".to_string())
            })
    }

    /// Perform all build-time generation: arg types, package.yaml, and
    /// skill/agent templates if configured. Reads `CARGO_MANIFEST_DIR`
    /// from the environment.
    pub fn finish(self) {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
        build(&self, Path::new(&manifest_dir));
    }
}

pub fn generate_package_yaml(spec: &PackageSpec) -> String {
    let mut yaml = String::new();

    yaml.push_str(&format!("name: {}\n", spec.info.name));
    yaml.push_str(&format!("version: \"{}\"\n", spec.info.version));
    yaml.push_str(&format!("binary: {}\n", spec.resolved_binary()));

    if let Some(description) = spec.info.description {
        yaml.push_str(&format!("description: \"{}\"\n", description.replace('"', "\\\"")));
    }

    yaml.push_str("protocol: true\n");

    if spec.project_data {
        yaml.push_str("project-data: true\n");
    }

    if !spec.package_depends.is_empty() || !spec.binary_depends.is_empty() {
        yaml.push_str("depends:\n");
        if !spec.package_depends.is_empty() {
            yaml.push_str("  packages:\n");
            for dep in &spec.package_depends {
                yaml.push_str(&format!("    - name: {}\n", dep.name));
                if let Some(version) = dep.version {
                    yaml.push_str(&format!("      version: \"{}\"\n", version));
                }
                if let Some(source) = dep.source {
                    yaml.push_str(&format!("      source: \"{}\"\n", source));
                }
            }
        }
        if !spec.binary_depends.is_empty() {
            yaml.push_str("  binaries:\n");
            for dep in &spec.binary_depends {
                yaml.push_str(&format!("    - binary: {}\n", dep.binary));
                if let Some(check) = dep.check {
                    yaml.push_str(&format!("      check: \"{}\"\n", check));
                }
                if let Some(install_hint) = dep.install_hint {
                    yaml.push_str(&format!("      install_hint: \"{}\"\n", install_hint));
                }
            }
        }
    }

    yaml.push_str("build:\n");
    yaml.push_str(&format!(
        "  command: \"{}\"\n",
        spec.resolved_build_command()
    ));
    yaml.push_str(&format!("  output: {}\n", spec.resolved_build_output()));

    yaml.push_str("commands:\n");

    for command in &spec.commands {
        let command_name = command.resolved_name(spec.commands.len());
        write_command_yaml(&mut yaml, command_name, command, 1);
    }

    if let Some(mode) = &spec.execution_default {
        yaml.push_str(&format!("execution:\n  default: {mode}\n"));
    }

    if let Some(svc) = &spec.service {
        yaml.push_str(&format!(
            "service:\n  start: \"{}\"\n  port: {}\n  health: {}\n",
            svc.start, svc.port, svc.health
        ));
    }

    yaml
}

fn write_command_yaml(yaml: &mut String, name: &str, command: &CommandSpec, depth: usize) {
    let indent = "  ".repeat(depth);
    let inner = "  ".repeat(depth + 1);
    let field_indent = "  ".repeat(depth + 2);

    yaml.push_str(&format!("{indent}{name}:\n"));

    if let Some(description) = command.description {
        yaml.push_str(&format!("{inner}description: \"{}\"\n", description.replace('"', "\\\"")));
    }

    if !command.subcommands.is_empty() {
        yaml.push_str(&format!("{inner}commands:\n"));
        for sub in &command.subcommands {
            let sub_name = sub.resolved_name(command.subcommands.len());
            write_command_yaml(yaml, sub_name, sub, depth + 2);
        }
        return;
    }

    if !command.args.is_empty() {
        yaml.push_str(&format!("{inner}args:\n"));
        for arg in &command.args {
            yaml.push_str(&format!("{field_indent}{}:\n", arg.name));
            yaml.push_str(&format!(
                "{field_indent}  type: {}\n",
                arg.arg_type.as_yaml()
            ));
            if arg.required {
                yaml.push_str(&format!("{field_indent}  required: true\n"));
            }
            if let Some(flag) = arg.flag {
                yaml.push_str(&format!("{field_indent}  flag: {flag}\n"));
            }
            if arg.rest {
                yaml.push_str(&format!("{field_indent}  rest: true\n"));
            }
            if let Some(default) = arg.default {
                match default {
                    DefaultValue::String(value) => {
                        yaml.push_str(&format!("{field_indent}  default: \"{value}\"\n"));
                    }
                    DefaultValue::Number(value) => {
                        yaml.push_str(&format!("{field_indent}  default: {value}\n"));
                    }
                    DefaultValue::Bool(value) => {
                        yaml.push_str(&format!("{field_indent}  default: {value}\n"));
                    }
                }
            }
        }
    }

    if let Some(input) = command.input {
        yaml.push_str(&format!("{inner}input: {}\n", input.as_yaml()));
    }
    if let Some(fallback) = command.inline_input_fallback {
        yaml.push_str(&format!(
            "{inner}inline-input-fallback: {}\n",
            fallback.as_yaml()
        ));
    }
    if let Some(output) = &command.output {
        let kind = output.resolved_kind();
        yaml.push_str(&format!("{inner}output:\n"));
        yaml.push_str(&format!("{field_indent}type: {}\n", kind.as_yaml()));
        if output.stream {
            yaml.push_str(&format!("{field_indent}stream: true\n"));
        }
        if let Some(field) = output.resolved_field(kind) {
            yaml.push_str(&format!("{field_indent}field: {field}\n"));
        }
        if !output.schema.is_empty() {
            yaml.push_str(&format!("{field_indent}schema:\n"));
            for field in &output.schema {
                yaml.push_str(&format!(
                    "{field_indent}  {}: {}\n",
                    field.name,
                    field.field_type.as_yaml()
                ));
            }
        }
    }
}

pub fn write_package_yaml(spec: &PackageSpec, output_dir: &Path) {
    write_package_yaml_file(spec, &output_dir.join("package.yaml"));
}

pub fn write_package_yaml_file(spec: &PackageSpec, output_path: &Path) {
    // The sidecar is always the canonical native view. Directory install
    // auto-detects a wasm artifact from cargo's target dir and prefers it
    // over the native binary when present (see `install_project` in
    // `zacor/src/source/local.rs`), so the sidecar doesn't need to flip
    // per target — keeping it stable avoids surprise when builds for
    // multiple targets race.
    let yaml = generate_package_yaml(spec);

    if let Ok(existing) = std::fs::read_to_string(output_path)
        && existing == yaml
    {
        return;
    }

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create package.yaml parent directory");
    }

    std::fs::write(output_path, &yaml).expect("failed to write package.yaml");
}

pub fn rerun_if_changed(src_dir: &Path) {
    println!("cargo:rerun-if-changed={}", src_dir.join("src").display());
}

// ---------------------------------------------------------------------------
// Arg type codegen
// ---------------------------------------------------------------------------

/// Generate typed arg structs and `FromArgs` implementations for every command
/// in the package spec. Writes `{out_dir}/args.rs`.
pub fn generate_arg_types(spec: &PackageSpec, out_dir: &str) {
    let mut code = String::new();
    code.push_str("// Generated by zacor-package build. Do not edit.\n");
    code.push_str("#[allow(unused_imports)]\n");
    code.push_str("use std::collections::BTreeMap;\n");
    code.push_str("#[allow(unused_imports)]\n");
    code.push_str("use std::path::PathBuf;\n\n");

    // Flatten commands including subcommands (e.g., topo.list, topo.show)
    let flat = flatten_commands(&spec.commands, "", spec.commands.len());
    for (full_name, cmd) in &flat {
        let struct_name = to_args_struct_name(full_name);

        // Struct definition
        code.push_str("#[derive(Debug)]\n");
        code.push_str(&format!("pub struct {} {{\n", struct_name));
        for arg in &cmd.args {
            let rust_type = arg_rust_type(arg);
            code.push_str(&format!(
                "    pub {}: {},\n",
                arg_field_name(arg.name),
                rust_type
            ));
        }
        code.push_str("}\n\n");

        // FromArgs impl
        code.push_str(&format!(
            "impl ::zacor_package::FromArgs for {} {{\n",
            struct_name
        ));
        let args_param = if cmd.args.is_empty() { "_args" } else { "args" };
        code.push_str(&format!(
            "    fn from_args({}: &BTreeMap<String, ::serde_json::Value>) -> Result<Self, String> {{\n",
            args_param
        ));
        for arg in &cmd.args {
            let field = arg_field_name(arg.name);
            generate_arg_parser(&mut code, arg, &field);
        }
        code.push_str("        Ok(Self {\n");
        for arg in &cmd.args {
            let field = arg_field_name(arg.name);
            code.push_str(&format!("            {},\n", field));
        }
        code.push_str("        })\n");
        code.push_str("    }\n");
        code.push_str("}\n\n");
    }

    let out_path = Path::new(out_dir).join("args.rs");
    write_if_changed(&out_path, &code);
}

/// Flatten commands and subcommands into (full_name, &CommandSpec) pairs.
/// Subcommands get dot-separated names like "topo.list".
fn flatten_commands<'a>(
    commands: &'a [CommandSpec],
    prefix: &str,
    command_count: usize,
) -> Vec<(String, &'a CommandSpec)> {
    let mut result = Vec::new();
    for cmd in commands {
        let name = cmd.resolved_name(command_count);
        let full_name = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}.{name}")
        };
        if cmd.subcommands.is_empty() {
            result.push((full_name, cmd));
        } else {
            let sub_count = cmd.subcommands.len();
            result.extend(flatten_commands(&cmd.subcommands, &full_name, sub_count));
        }
    }
    result
}

/// Convert a command name to a PascalCase struct name with "Args" suffix.
/// Splits on `-` and `.` so "topo.list" becomes "TopoListArgs".
fn to_args_struct_name(cmd_name: &str) -> String {
    let pascal: String = cmd_name
        .split(['-', '.'])
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    format!("{}Args", pascal)
}

/// Convert an arg name (kebab-case) to a Rust field name (snake_case).
fn arg_field_name(name: &str) -> String {
    name.replace('-', "_")
}

/// Determine the Rust type for an arg based on its schema info.
fn arg_rust_type(arg: &ArgSchemaInfo) -> String {
    let base = match arg.arg_type {
        ValueKind::Bool => return "bool".to_string(), // bools always default to false
        ValueKind::Number => "f64",
        ValueKind::Integer => "i64",
        ValueKind::Path => "PathBuf",
        _ => "String",
    };

    let is_optional = !arg.required && arg.default.is_none();
    if is_optional {
        format!("Option<{}>", base)
    } else {
        base.to_string()
    }
}

/// Generate the parser code for a single arg field.
fn generate_arg_parser(code: &mut String, arg: &ArgSchemaInfo, field: &str) {
    let name = arg.name;
    match arg.arg_type {
        ValueKind::Bool => {
            let default = match arg.default {
                Some(DefaultValue::Bool(b)) => b,
                _ => false,
            };
            code.push_str(&format!(
                "        let {} = match args.get(\"{}\") {{\n\
                 \x20           Some(::serde_json::Value::Bool(b)) => *b,\n\
                 \x20           Some(::serde_json::Value::String(s)) => s == \"true\",\n\
                 \x20           _ => {},\n\
                 \x20       }};\n",
                field, name, default
            ));
        }
        ValueKind::Number => {
            let is_optional = !arg.required && arg.default.is_none();
            if is_optional {
                code.push_str(&format!(
                    "        let {} = match args.get(\"{}\") {{\n\
                     \x20           Some(::serde_json::Value::Number(n)) => n.as_f64(),\n\
                     \x20           Some(::serde_json::Value::String(s)) => s.parse().ok(),\n\
                     \x20           _ => None,\n\
                     \x20       }};\n",
                    field, name
                ));
            } else if arg.required {
                code.push_str(&format!(
                    "        let {} = match args.get(\"{}\") {{\n\
                     \x20           Some(::serde_json::Value::Number(n)) => n.as_f64()\n\
                     \x20               .ok_or_else(|| \"missing required arg: {}\".to_string())?,\n\
                     \x20           Some(::serde_json::Value::String(s)) => s.parse()\n\
                     \x20               .map_err(|_| format!(\"arg '{}' is not a valid number\"))?,\n\
                     \x20           Some(_) => return Err(\"arg '{}' is not a valid number\".to_string()),\n\
                     \x20           None => return Err(\"missing required arg: {}\".to_string()),\n\
                     \x20       }};\n",
                    field, name, name, name, name, name
                ));
            } else {
                // has default
                let default = match arg.default {
                    Some(DefaultValue::Number(n)) => format!("{}.0", n),
                    _ => "0.0".to_string(),
                };
                code.push_str(&format!(
                    "        let {} = match args.get(\"{}\") {{\n\
                     \x20           Some(::serde_json::Value::Number(n)) => n.as_f64().unwrap_or({}),\n\
                     \x20           Some(::serde_json::Value::String(s)) => s.parse().unwrap_or({}),\n\
                     \x20           _ => {},\n\
                     \x20       }};\n",
                    field, name, default, default, default
                ));
            }
        }
        ValueKind::Integer => {
            let is_optional = !arg.required && arg.default.is_none();
            if is_optional {
                code.push_str(&format!(
                    "        let {} = match args.get(\"{}\") {{\n\
                     \x20           Some(::serde_json::Value::Number(n)) => n.as_i64(),\n\
                     \x20           Some(::serde_json::Value::String(s)) => s.parse().ok(),\n\
                     \x20           _ => None,\n\
                     \x20       }};\n",
                    field, name
                ));
            } else if arg.required {
                code.push_str(&format!(
                    "        let {} = match args.get(\"{}\") {{\n\
                     \x20           Some(::serde_json::Value::Number(n)) => n.as_i64()\n\
                     \x20               .ok_or_else(|| \"missing required arg: {}\".to_string())?,\n\
                     \x20           Some(::serde_json::Value::String(s)) => s.parse()\n\
                     \x20               .map_err(|_| format!(\"arg '{}' is not a valid integer\"))?,\n\
                     \x20           Some(_) => return Err(\"arg '{}' is not a valid integer\".to_string()),\n\
                     \x20           None => return Err(\"missing required arg: {}\".to_string()),\n\
                     \x20       }};\n",
                    field, name, name, name, name, name
                ));
            } else {
                // has default
                let default = match arg.default {
                    Some(DefaultValue::Number(n)) => format!("{}", n),
                    _ => "0".to_string(),
                };
                code.push_str(&format!(
                    "        let {} = match args.get(\"{}\") {{\n\
                     \x20           Some(::serde_json::Value::Number(n)) => n.as_i64().unwrap_or({}),\n\
                     \x20           Some(::serde_json::Value::String(s)) => s.parse().unwrap_or({}),\n\
                     \x20           _ => {},\n\
                     \x20       }};\n",
                    field, name, default, default, default
                ));
            }
        }
        ValueKind::Path => {
            let is_optional = !arg.required && arg.default.is_none();
            if is_optional {
                code.push_str(&format!(
                    "        let {} = args.get(\"{}\").and_then(|v| v.as_str()).map(PathBuf::from);\n",
                    field, name
                ));
            } else if arg.required {
                code.push_str(&format!(
                    "        let {} = args.get(\"{}\")\n\
                     \x20           .and_then(|v| v.as_str())\n\
                     \x20           .map(PathBuf::from)\n\
                     \x20           .ok_or_else(|| \"missing required arg: {}\".to_string())?;\n",
                    field, name, name
                ));
            } else {
                let default = match arg.default {
                    Some(DefaultValue::String(s)) => format!("\"{}\"", s),
                    _ => "\".\"".to_string(),
                };
                code.push_str(&format!(
                    "        let {} = args.get(\"{}\").and_then(|v| v.as_str()).map(PathBuf::from).unwrap_or_else(|| PathBuf::from({}));\n",
                    field, name, default
                ));
            }
        }
        _ => {
            // String and other types
            let is_optional = !arg.required && arg.default.is_none();
            if is_optional {
                code.push_str(&format!(
                    "        let {} = match args.get(\"{}\") {{\n\
                     \x20           Some(::serde_json::Value::String(s)) => Some(s.clone()),\n\
                     \x20           Some(::serde_json::Value::Number(n)) => Some(n.to_string()),\n\
                     \x20           Some(::serde_json::Value::Bool(b)) => Some(b.to_string()),\n\
                     \x20           _ => None,\n\
                     \x20       }};\n",
                    field, name
                ));
            } else if arg.required {
                code.push_str(&format!(
                    "        let {} = match args.get(\"{}\") {{\n\
                     \x20           Some(::serde_json::Value::String(s)) => s.clone(),\n\
                     \x20           Some(::serde_json::Value::Number(n)) => n.to_string(),\n\
                     \x20           Some(::serde_json::Value::Bool(b)) => b.to_string(),\n\
                     \x20           Some(_) => return Err(\"arg '{}' has unsupported type\".to_string()),\n\
                     \x20           None => return Err(\"missing required arg: {}\".to_string()),\n\
                     \x20       }};\n",
                    field, name, name, name
                ));
            } else {
                let default = match arg.default {
                    Some(DefaultValue::String(s)) => format!("\"{}\".to_string()", s),
                    _ => "String::new()".to_string(),
                };
                code.push_str(&format!(
                    "        let {} = match args.get(\"{}\") {{\n\
                     \x20           Some(::serde_json::Value::String(s)) => s.clone(),\n\
                     \x20           Some(::serde_json::Value::Number(n)) => n.to_string(),\n\
                     \x20           Some(::serde_json::Value::Bool(b)) => b.to_string(),\n\
                     \x20           _ => {},\n\
                     \x20       }};\n",
                    field, name, default
                ));
            }
        }
    }
}

/// Write a file only if its content has changed.
fn write_if_changed(path: &Path, content: &str) {
    if let Ok(existing) = std::fs::read_to_string(path)
        && existing == content
    {
        return;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create output directory");
    }
    std::fs::write(path, content).expect("failed to write generated file");
}

// ---------------------------------------------------------------------------
// Skill / Agent / Template codegen
// ---------------------------------------------------------------------------

/// All-in-one build entry point. Runs codegen for skills/agents/templates
/// (if configured via `.skills()` / `.agents()`), generates arg types,
/// writes package.yaml, and emits cargo rerun-if-changed directives.
pub fn build(spec: &PackageSpec, manifest_dir: &Path) {
    if let Some(ref config) = spec.skills_config {
        let prompts_dir = manifest_dir.join(config.prompts_dir);
        let templates_dir = manifest_dir.join(config.templates_dir);
        generate_skill_templates_codegen(&prompts_dir, &templates_dir);
        generate_template_serve_codegen(&templates_dir);
        println!("cargo::rerun-if-changed={}", prompts_dir.display());
        println!("cargo::rerun-if-changed={}", templates_dir.display());
    }

    if let Some(ref config) = spec.agents_config {
        let prompts_dir = manifest_dir.join(config.prompts_dir);
        let templates_dir = manifest_dir.join(config.templates_dir);
        generate_agent_templates_codegen(&prompts_dir, &templates_dir);
        println!("cargo::rerun-if-changed={}", prompts_dir.display());
    }

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    generate_arg_types(spec, &out_dir);
    generate_embedded_manifest(spec, &out_dir);

    write_package_yaml(spec, manifest_dir);
    rerun_if_changed(manifest_dir);
}

/// Generate a pair of files in OUT_DIR that embed the generated package
/// manifest as a wasm custom section named `zacor_manifest`.
///
/// - `package_manifest.yaml` — the yaml bytes (same content as the
///   sibling `package.yaml` written to the crate root).
/// - `manifest.rs` — a Rust source file that, when included via
///   `zacor_package::include_args!()`, declares a `#[link_section]`
///   static on wasm targets so the linker places the yaml bytes in
///   a custom section of the final `.wasm` artifact.
///
/// The `#[link_section]` / `#[used]` pair is gated to wasm families —
/// native builds don't need this (they continue to use the sidecar
/// `package.yaml` on disk) and some native linkers reject arbitrary
/// section names.
/// Transform a native-view package.yaml string into the wasm-target variant:
/// - `binary: foo` → `wasm: foo.wasm` (cargo's wasm32 output filename).
/// - `  output: <prefix>/target/release` → `.../target/wasm32-wasip1/release`
///   so directory-based install can locate the pre-built wasm artifact.
/// Other lines pass through unchanged.
fn embedded_manifest_yaml(native_yaml: &str) -> String {
    let mut out = String::with_capacity(native_yaml.len());
    for line in native_yaml.lines() {
        let rewritten = if let Some(rest) = line.strip_prefix("binary: ") {
            format!("wasm: {}.wasm", rest.trim())
        } else if let Some(rest) = line.strip_prefix("  output: ") {
            let path = rest.trim();
            let rewritten_path = if path == "target/release" {
                "target/wasm32-wasip1/release".to_string()
            } else if let Some(prefix) = path.strip_suffix("/target/release") {
                format!("{prefix}/target/wasm32-wasip1/release")
            } else {
                path.to_string()
            };
            format!("  output: {}", rewritten_path)
        } else {
            line.to_string()
        };
        out.push_str(&rewritten);
        out.push('\n');
    }
    out
}

pub fn generate_embedded_manifest(spec: &PackageSpec, out_dir: &str) {
    // The embedded manifest describes the wasm artifact, so rewrite
    // the native `binary: <name>` line to `wasm: <name>.wasm` to match
    // cargo's wasm output filename. The sidecar `package.yaml` at the
    // crate root mirrors this rewrite when built for a wasm target
    // (see `generate_sidecar_yaml`).
    let native_yaml = generate_package_yaml(spec);
    let yaml = embedded_manifest_yaml(&native_yaml);
    let yaml_len = yaml.as_bytes().len();
    let yaml_path = Path::new(out_dir).join("package_manifest.yaml");
    write_if_changed(&yaml_path, &yaml);

    let rs = format!(
        r#"// Generated by zacor-package-build. Do not edit.
#[cfg(target_family = "wasm")]
#[unsafe(link_section = "zacor_manifest")]
#[used]
pub static ZACOR_MANIFEST: [u8; {len}] =
    *include_bytes!(concat!(env!("OUT_DIR"), "/package_manifest.yaml"));
"#,
        len = yaml_len
    );
    let rs_path = Path::new(out_dir).join("manifest.rs");
    write_if_changed(&rs_path, &rs);
}

/// Parse YAML frontmatter delimited by `---` lines. Returns (key-value map, body).
pub fn parse_skill_frontmatter(content: &str) -> (BTreeMap<String, String>, String) {
    let mut fm = BTreeMap::new();

    if !content.starts_with("---") {
        return (fm, content.to_string());
    }

    let after_open = &content[3..];
    let Some(close_pos) = after_open.find("\n---") else {
        return (fm, content.to_string());
    };

    let fm_block = &after_open[..close_pos];
    let body = after_open[close_pos + 4..]
        .trim_start_matches('\n')
        .to_string();

    for line in fm_block.lines() {
        let line = line.trim();
        if line.is_empty() || !line.contains(':') {
            continue;
        }
        let (key, val) = line.split_once(':').unwrap();
        let key = key.trim().to_string();
        let val = val.trim().trim_matches('"').to_string();
        fm.insert(key, val);
    }

    (fm, body)
}

/// Resolve local `{{name}}` template references by inlining content from
/// `templates_dir/{name}.md`. Cross-package references (`{{pkg.name}}`)
/// are left as markers for runtime resolution.
pub fn resolve_templates(body: &str, templates_dir: &Path, prompt_file: &str) -> String {
    let mut result = body.to_string();
    loop {
        let Some(start) = result.find("{{") else {
            break;
        };
        let Some(rel_end) = result[start..].find("}}") else {
            panic!(
                "Prompt '{}': unclosed '{{{{' at byte {}",
                prompt_file, start
            );
        };
        let end = start + rel_end + 2;
        let name = result[start + 2..end - 2].trim();

        // Cross-package reference (contains '.') — leave as marker for runtime
        if name.contains('.') {
            // Move past this marker so we don't loop forever
            let marker = &result[start..end].to_string();
            let placeholder = format!("\x00XPKG{}\x00", &marker[2..marker.len() - 2]);
            result.replace_range(start..end, &placeholder);
            continue;
        }

        let template_path = templates_dir.join(format!("{name}.md"));
        if !template_path.is_file() {
            panic!(
                "Prompt '{}' references template '{}' but {} does not exist",
                prompt_file,
                name,
                template_path.display()
            );
        }
        let template_body = std::fs::read_to_string(&template_path).unwrap();
        result.replace_range(start..end, &template_body);
    }

    // Restore cross-package markers
    while let Some(pos) = result.find("\x00XPKG") {
        if let Some(e) = result[pos + 5..].find('\x00').map(|i| pos + 5 + i) {
            let name = &result[pos + 5..e].to_string();
            result.replace_range(pos..e + 1, &format!("{{{{{name}}}}}"));
        } else {
            break;
        }
    }

    result
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn validate_skill_prompt_or_panic(prompt_file: &str, body: &str) {
    let diagnostics = zacor_package::skills::validate_skill_prompt_body(prompt_file, body);
    if let Some(diag) = diagnostics.into_iter().next() {
        panic!("Skill prompt {} {}", diag.prompt_file, diag.message());
    }
}

fn generate_skill_templates_codegen(prompts_dir: &Path, templates_dir: &Path) {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    generate_skill_templates_codegen_at(prompts_dir, templates_dir, Path::new(&out_dir));
}

fn generate_skill_templates_codegen_at(prompts_dir: &Path, templates_dir: &Path, out_dir: &Path) {
    use std::fs;

    let dest = out_dir.join("skill_templates_gen.rs");

    let mut entries: Vec<(String, BTreeMap<String, String>, String)> = Vec::new();

    if prompts_dir.is_dir() {
        for entry in fs::read_dir(prompts_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "md") {
                continue;
            }

            let content = fs::read_to_string(&path).unwrap();
            let (fm, body) = parse_skill_frontmatter(&content);

            if !fm.contains_key("name") {
                panic!(
                    "Skill prompt {} missing 'name' in frontmatter",
                    path.display()
                );
            }

            let filename = path.file_name().unwrap().to_str().unwrap().to_string();
            let composed = resolve_templates(&body, templates_dir, &filename);
            validate_skill_prompt_or_panic(&filename, &composed);
            entries.push((filename, fm, composed));
        }
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut code = String::new();
    code.push_str("/// Auto-generated by zacor_package::build from skill prompts.\n");
    code.push_str("pub fn all() -> Vec<zacor_package::skills::SkillTemplate> {\n    vec![\n");

    for (_filename, fm, body) in &entries {
        let name = &fm["name"];
        let description = fm.get("description").map(|s| s.as_str()).unwrap_or("");
        let hint = fm.get("argument-hint");
        let tools = fm.get("allowed-tools");
        let effort = fm.get("effort");

        code.push_str("        zacor_package::skills::SkillTemplate {\n");
        code.push_str(&format!("            name: \"{}\".into(),\n", escape(name)));
        code.push_str(&format!(
            "            description: \"{}\".into(),\n",
            escape(description)
        ));
        emit_option(&mut code, "argument_hint", hint);
        emit_option(&mut code, "allowed_tools", tools);
        emit_option(&mut code, "effort", effort);
        code.push_str(&format!(
            "            prompt: r###\"{}\"###.into(),\n",
            body
        ));
        code.push_str("        },\n");
    }

    code.push_str("    ]\n}\n");
    fs::write(&dest, code).unwrap();
}

fn generate_agent_templates_codegen(prompts_dir: &Path, templates_dir: &Path) {
    use std::fs;

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("agent_templates_gen.rs");

    let mut entries: Vec<(String, BTreeMap<String, String>, String)> = Vec::new();

    if prompts_dir.is_dir() {
        for entry in fs::read_dir(prompts_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "md") {
                continue;
            }

            let content = fs::read_to_string(&path).unwrap();
            let (fm, body) = parse_skill_frontmatter(&content);

            if !fm.contains_key("name") {
                panic!(
                    "Agent prompt {} missing 'name' in frontmatter",
                    path.display()
                );
            }

            let filename = path.file_name().unwrap().to_str().unwrap().to_string();
            let composed = resolve_templates(&body, templates_dir, &filename);
            entries.push((filename, fm, composed));
        }
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut code = String::new();
    code.push_str("/// Auto-generated by zacor_package::build from agent prompts.\n");
    code.push_str("pub fn all() -> Vec<zacor_package::skills::AgentTemplate> {\n    vec![\n");

    for (_filename, fm, body) in &entries {
        let name = &fm["name"];
        let description = fm.get("description").map(|s| s.as_str()).unwrap_or("");
        let tools = fm.get("tools");
        let model = fm.get("model");

        code.push_str("        zacor_package::skills::AgentTemplate {\n");
        code.push_str(&format!("            name: \"{}\".into(),\n", escape(name)));
        code.push_str(&format!(
            "            description: \"{}\".into(),\n",
            escape(description)
        ));
        emit_option(&mut code, "tools", tools);
        emit_option(&mut code, "model", model);
        code.push_str(&format!(
            "            prompt: r###\"{}\"###.into(),\n",
            body
        ));
        code.push_str("        },\n");
    }

    code.push_str("    ]\n}\n");
    fs::write(&dest, code).unwrap();
}

fn generate_template_serve_codegen(templates_dir: &Path) {
    use std::fs;

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("template_serve_gen.rs");

    let mut entries: Vec<(String, String)> = Vec::new();

    if templates_dir.is_dir() {
        for entry in fs::read_dir(templates_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "md") {
                continue;
            }
            let name = path.file_stem().unwrap().to_str().unwrap().to_string();
            let content = fs::read_to_string(&path).unwrap();
            entries.push((name, content));
        }
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut code = String::new();
    code.push_str("/// Auto-generated by zacor_package::build from template files.\n");
    code.push_str("pub fn all() -> Vec<(&'static str, &'static str)> {\n    vec![\n");

    for (name, content) in &entries {
        code.push_str(&format!(
            "        (\"{}\", r###\"{}\"###),\n",
            escape(name),
            content
        ));
    }

    code.push_str("    ]\n}\n");
    fs::write(&dest, code).unwrap();
}

fn emit_option(code: &mut String, field: &str, val: Option<&String>) {
    match val {
        Some(v) => code.push_str(&format!(
            "            {field}: Some(\"{}\".into()),\n",
            escape(v)
        )),
        None => code.push_str(&format!("            {field}: None,\n")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT_ARG: [ArgSchemaInfo; 1] = [ArgSchemaInfo {
        name: "text",
        arg_type: ValueKind::String,
        required: false,
        flag: None,
        default: None,
        rest: false,
    }];

    const TEXT_FIELDS: [FieldSchemaInfo; 1] = [FieldSchemaInfo {
        name: "text",
        field_type: ValueKind::String,
    }];

    const LS_ARGS: [ArgSchemaInfo; 2] = [
        ArgSchemaInfo {
            name: "path",
            arg_type: ValueKind::Path,
            required: false,
            flag: None,
            default: Some(DefaultValue::String(".")),
            rest: false,
        },
        ArgSchemaInfo {
            name: "all",
            arg_type: ValueKind::Bool,
            required: false,
            flag: Some("a"),
            default: None,
            rest: false,
        },
    ];

    const LS_FIELDS: [FieldSchemaInfo; 3] = [
        FieldSchemaInfo {
            name: "name",
            field_type: ValueKind::String,
        },
        FieldSchemaInfo {
            name: "size",
            field_type: ValueKind::Filesize,
        },
        FieldSchemaInfo {
            name: "kind",
            field_type: ValueKind::String,
        },
    ];

    #[test]
    fn test_generate_basic_yaml_uses_defaults() {
        let spec = PackageSpec::new("echo", "0.2.0")
            .description("Display text")
            .command(
                CommandSpec::implicit_default()
                    .description("Echo text to stdout")
                    .args(&TEXT_ARG)
                    .output(OutputSpec::infer(&TEXT_FIELDS)),
            );

        let yaml = generate_package_yaml(&spec);
        assert!(yaml.contains("binary: echo"));
        assert!(yaml.contains("command: \"cargo build --release --bin echo\""));
        // In a workspace context, auto-detects "../target/release"; standalone uses "target/release"
        assert!(yaml.contains("output: ") && (yaml.contains("target/release")));
        assert!(yaml.contains("  default:"));
        assert!(yaml.contains("type: text"));
        assert!(yaml.contains("field: text"));
    }

    #[test]
    fn test_generate_yaml_with_args_and_flags() {
        let spec = PackageSpec::new("ls", "0.2.0")
            .description("List directory contents")
            .command(
                CommandSpec::implicit_default()
                    .description("List directory entries")
                    .args(&LS_ARGS)
                    .output(OutputSpec::table(&LS_FIELDS)),
            );

        let yaml = generate_package_yaml(&spec);
        assert!(yaml.contains("flag: a"));
        assert!(yaml.contains("default: \".\""));
        assert!(yaml.contains("size: filesize"));
        assert!(yaml.contains("type: table"));
    }

    #[test]
    fn test_generate_yaml_with_stream() {
        let fields = [
            FieldSchemaInfo::number("line"),
            FieldSchemaInfo::string("content"),
        ];
        let spec = PackageSpec::new("cat", "0.2.0")
            .description("Concatenate and display files")
            .command(
                CommandSpec::implicit_default()
                    .description("Output file contents as line records")
                    .args(&[ArgSchemaInfo::path("file").optional()])
                    .output(OutputSpec::streaming_table(&fields)),
            );

        let yaml = generate_package_yaml(&spec);
        assert!(yaml.contains("stream: true"));
    }

    #[test]
    fn test_generate_yaml_with_default_value() {
        let fields = [
            FieldSchemaInfo::number("line"),
            FieldSchemaInfo::string("content"),
        ];
        let spec = PackageSpec::new("head", "0.2.0")
            .description("Output first lines of a file")
            .command(
                CommandSpec::implicit_default()
                    .description("Show first N lines")
                    .args(&[
                        ArgSchemaInfo::path("file").optional(),
                        ArgSchemaInfo::number("lines").default(DefaultValue::Number(10)),
                    ])
                    .output(OutputSpec::streaming_table(&fields)),
            );

        let yaml = generate_package_yaml(&spec);
        assert!(yaml.contains("default: 10"));
    }

    #[test]
    fn sidecar_yaml_is_always_native() {
        // The sidecar stays stable as the native view regardless of the
        // target family being built. Directory install auto-detects the
        // wasm artifact separately.
        let spec = PackageSpec::new("echo", "0.2.0")
            .command(CommandSpec::implicit_default().description("Echo"));
        let yaml = generate_package_yaml(&spec);
        assert!(yaml.contains("binary: echo"));
        assert!(!yaml.contains("wasm:"));
    }

    #[test]
    fn test_project_data_true() {
        let spec = PackageSpec::new("wf", "0.1.0")
            .project_data()
            .command(CommandSpec::named("status").description("Show status"));
        let yaml = generate_package_yaml(&spec);
        assert!(yaml.contains("project-data: true\n"));
    }

    #[test]
    fn test_project_data_false_omitted() {
        let spec = PackageSpec::new("echo", "0.2.0")
            .command(CommandSpec::implicit_default().description("Echo"));
        let yaml = generate_package_yaml(&spec);
        assert!(!yaml.contains("project-data"));
    }

    #[test]
    fn test_depends_omitted_when_empty() {
        let spec = PackageSpec::new("echo", "0.2.0")
            .command(CommandSpec::implicit_default().description("Echo"));
        let yaml = generate_package_yaml(&spec);
        assert!(!yaml.contains("depends:\n"));
    }

    #[test]
    fn test_package_depends_yaml() {
        let spec = PackageSpec::new("wf", "0.1.0")
            .depends_package(PackageDependency::named("mermaid"))
            .depends_package(
                PackageDependency::named("treesitter")
                    .version("0.2.0")
                    .source("file:///packages/treesitter"),
            )
            .command(CommandSpec::named("status").description("Show status"));
        let yaml = generate_package_yaml(&spec);
        assert!(yaml.contains("depends:\n"));
        assert!(yaml.contains("  packages:\n"));
        assert!(yaml.contains("    - name: mermaid\n"));
        assert!(yaml.contains("    - name: treesitter\n"));
        assert!(yaml.contains("      version: \"0.2.0\"\n"));
        assert!(yaml.contains("      source: \"file:///packages/treesitter\"\n"));
    }

    #[test]
    fn test_binary_depends_yaml() {
        let spec = PackageSpec::new("video", "0.1.0")
            .depends_binary(
                BinaryDependency::named("ffmpeg")
                    .check("ffmpeg -version")
                    .install_hint("Install ffmpeg from https://ffmpeg.org"),
            )
            .command(CommandSpec::implicit_default().description("Render video"));
        let yaml = generate_package_yaml(&spec);
        assert!(yaml.contains("depends:\n"));
        assert!(yaml.contains("  binaries:\n"));
        assert!(yaml.contains("    - binary: ffmpeg\n"));
        assert!(yaml.contains("      check: \"ffmpeg -version\"\n"));
        assert!(yaml.contains("      install_hint: \"Install ffmpeg from https://ffmpeg.org\"\n"));
    }

    #[test]
    fn test_embedded_manifest_preserves_depends() {
        let out = tempfile::tempdir().unwrap();
        let spec = PackageSpec::new("wf", "0.1.0")
            .depends_package(PackageDependency::named("mermaid"))
            .depends_binary(BinaryDependency::named("dot").check("dot -V"))
            .command(CommandSpec::implicit_default().description("Render docs"));

        generate_embedded_manifest(&spec, out.path().to_str().unwrap());

        let yaml = std::fs::read_to_string(out.path().join("package_manifest.yaml")).unwrap();
        assert!(yaml.contains("depends:\n"));
        assert!(yaml.contains("  packages:\n    - name: mermaid\n"));
        assert!(yaml.contains("  binaries:\n    - binary: dot\n      check: \"dot -V\"\n"));
    }

    #[test]
    fn test_subcommands_yaml() {
        let spec = PackageSpec::new("wf", "0.1.0").command(
            CommandSpec::named("topo")
                .description("Topo doc commands")
                .subcommand(
                    CommandSpec::named("list")
                        .description("List topo docs")
                        .output(OutputSpec::table(&[FieldSchemaInfo::string("name")])),
                )
                .subcommand(
                    CommandSpec::named("show")
                        .description("Show a topo doc")
                        .args(&[ArgSchemaInfo::string("name").required()])
                        .output(OutputSpec::record(&[FieldSchemaInfo::string("content")])),
                ),
        );
        let yaml = generate_package_yaml(&spec);
        assert!(yaml.contains("  topo:\n"));
        assert!(yaml.contains("    description: \"Topo doc commands\"\n"));
        assert!(yaml.contains("    commands:\n"));
        assert!(yaml.contains("      list:\n"));
        assert!(yaml.contains("        description: \"List topo docs\"\n"));
        assert!(yaml.contains("      show:\n"));
        assert!(yaml.contains("        description: \"Show a topo doc\"\n"));
        assert!(yaml.contains("          name:\n"));
        assert!(yaml.contains("            type: string\n"));
        assert!(yaml.contains("            required: true\n"));
    }

    #[test]
    fn test_subcommand_parent_has_no_args_or_output() {
        let spec = PackageSpec::new("wf", "0.1.0").command(
            CommandSpec::named("topo")
                .description("Topo doc commands")
                .subcommand(CommandSpec::named("list").description("List")),
        );
        let yaml = generate_package_yaml(&spec);
        let topo_section: &str = yaml.split("  topo:\n").nth(1).unwrap();
        let topo_block = topo_section.split("\n  ").next().unwrap_or(topo_section);
        assert!(!topo_block.starts_with("    args:"));
        assert!(!topo_block.starts_with("    output:"));
    }

    #[test]
    fn test_skills_injects_skill_and_template_commands() {
        let spec = PackageSpec::new("wf", "0.1.0")
            .skills("src/skills/prompts", "src/templates")
            .command(CommandSpec::named("status").description("Show status"));

        let yaml = generate_package_yaml(&spec);
        assert!(yaml.contains("  skill:\n"));
        assert!(yaml.contains("    description: \"Serve a resolved skill prompt\"\n"));
        assert!(yaml.contains("  template:\n"));
        assert!(yaml.contains("    description: \"Serve a named reusable template\"\n"));
        // User command should also be present
        assert!(yaml.contains("  status:\n"));
    }

    #[test]
    fn test_skills_command_has_correct_args() {
        let spec = PackageSpec::new("wf", "0.1.0").skills("src/skills/prompts", "src/templates");

        let yaml = generate_package_yaml(&spec);
        // skill command has name (required) and args (positional) params
        assert!(yaml.contains("required: true"));
        // template command has list flag
        assert!(yaml.contains("flag: list"));
    }

    #[test]
    fn test_agents_does_not_inject_commands() {
        let spec = PackageSpec::new("wf", "0.1.0")
            .agents("src/agents/prompts", "src/templates")
            .command(CommandSpec::named("status").description("Show status"));

        let yaml = generate_package_yaml(&spec);
        assert!(!yaml.contains("  skill:\n"));
        assert!(!yaml.contains("  template:\n"));
        assert!(yaml.contains("  status:\n"));
    }

    #[test]
    fn test_parse_frontmatter_basic() {
        let content = "---\nname: search-web\ndescription: Search the web\n---\nPrompt body here";
        let (fm, body) = parse_skill_frontmatter(content);
        assert_eq!(fm["name"], "search-web");
        assert_eq!(fm["description"], "Search the web");
        assert_eq!(body, "Prompt body here");
    }

    #[test]
    fn test_parse_frontmatter_with_quotes() {
        let content = "---\nname: \"my-skill\"\n---\nBody";
        let (fm, body) = parse_skill_frontmatter(content);
        assert_eq!(fm["name"], "my-skill");
        assert_eq!(body, "Body");
    }

    #[test]
    fn test_parse_frontmatter_no_frontmatter() {
        let content = "Just a body with no frontmatter";
        let (fm, body) = parse_skill_frontmatter(content);
        assert!(fm.is_empty());
        assert_eq!(body, content);
    }

    #[test]
    fn test_resolve_templates_local() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("greeting.md"), "Hello world").unwrap();
        let result = resolve_templates("Before {{greeting}} after", dir.path(), "test.md");
        assert_eq!(result, "Before Hello world after");
    }

    #[test]
    fn test_resolve_templates_cross_package_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let result = resolve_templates("Use {{otherpkg.shared}} here", dir.path(), "test.md");
        assert_eq!(result, "Use {{otherpkg.shared}} here");
    }

    #[test]
    #[should_panic(expected = "does not exist")]
    fn test_resolve_templates_missing_local_panics() {
        let dir = tempfile::tempdir().unwrap();
        resolve_templates("Use {{nonexistent}} here", dir.path(), "test.md");
    }

    #[test]
    fn test_resolve_templates_mixed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("local.md"), "LOCAL").unwrap();
        let result = resolve_templates("A {{local}} B {{pkg.remote}} C", dir.path(), "test.md");
        assert_eq!(result, "A LOCAL B {{pkg.remote}} C");
    }

    #[test]
    fn validate_skill_prompt_codegen_accepts_valid_prompt() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("prompts")).unwrap();
        std::fs::create_dir_all(dir.path().join("templates")).unwrap();
        std::fs::write(dir.path().join("templates/mode.md"), "<mode>Read</mode>\n").unwrap();
        std::fs::write(
            dir.path().join("prompts/test.md"),
            "---\nname: test\n---\n<identity>Test</identity>\n<input>$ARGUMENTS</input>\n{{mode}}\n<strategy>Think</strategy>\n",
        )
        .unwrap();

        let out = tempfile::tempdir().unwrap();
        generate_skill_templates_codegen_at(
            &dir.path().join("prompts"),
            &dir.path().join("templates"),
            out.path(),
        );

        let generated = std::fs::read_to_string(out.path().join("skill_templates_gen.rs")).unwrap();
        assert!(generated.contains("<mode>Read</mode>"));
    }

    #[test]
    #[should_panic(expected = "disallowed tag '<topic>'")]
    fn validate_skill_prompt_codegen_rejects_disallowed_tag() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("prompts")).unwrap();
        std::fs::create_dir_all(dir.path().join("templates")).unwrap();
        std::fs::write(
            dir.path().join("prompts/test.md"),
            "---\nname: test\n---\n<identity>Test</identity>\n<input>$ARGUMENTS</input>\n<topic>bad</topic>\n",
        )
        .unwrap();

        let out = tempfile::tempdir().unwrap();
        generate_skill_templates_codegen_at(
            &dir.path().join("prompts"),
            &dir.path().join("templates"),
            out.path(),
        );
    }

    #[test]
    #[should_panic(expected = "missing required tag '<identity>'")]
    fn validate_skill_prompt_codegen_rejects_missing_required_tag() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("prompts")).unwrap();
        std::fs::create_dir_all(dir.path().join("templates")).unwrap();
        std::fs::write(
            dir.path().join("prompts/test.md"),
            "---\nname: test\n---\n<input>$ARGUMENTS</input>\n",
        )
        .unwrap();

        let out = tempfile::tempdir().unwrap();
        generate_skill_templates_codegen_at(
            &dir.path().join("prompts"),
            &dir.path().join("templates"),
            out.path(),
        );
    }

    #[test]
    #[should_panic(expected = "disallowed tag '<topic>'")]
    fn validate_skill_prompt_codegen_rejects_bad_local_template() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("prompts")).unwrap();
        std::fs::create_dir_all(dir.path().join("templates")).unwrap();
        std::fs::write(dir.path().join("templates/bad.md"), "<topic>bad</topic>\n").unwrap();
        std::fs::write(
            dir.path().join("prompts/test.md"),
            "---\nname: test\n---\n<identity>Test</identity>\n<input>$ARGUMENTS</input>\n{{bad}}\n",
        )
        .unwrap();

        let out = tempfile::tempdir().unwrap();
        generate_skill_templates_codegen_at(
            &dir.path().join("prompts"),
            &dir.path().join("templates"),
            out.path(),
        );
    }

    #[test]
    fn validate_skill_prompt_codegen_allows_cross_package_markers() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("prompts")).unwrap();
        std::fs::create_dir_all(dir.path().join("templates")).unwrap();
        std::fs::write(
            dir.path().join("prompts/test.md"),
            "---\nname: test\n---\n<identity>Test</identity>\n<input>$ARGUMENTS</input>\n{{otherpkg.shared}}\n",
        )
        .unwrap();

        let out = tempfile::tempdir().unwrap();
        generate_skill_templates_codegen_at(
            &dir.path().join("prompts"),
            &dir.path().join("templates"),
            out.path(),
        );

        let generated = std::fs::read_to_string(out.path().join("skill_templates_gen.rs")).unwrap();
        assert!(generated.contains("{{otherpkg.shared}}"));
    }

    #[test]
    fn from_cargo_reads_env() {
        // CARGO_PKG_VERSION is set by cargo during test runs
        let spec = PackageSpec::from_cargo("test-pkg");
        assert_eq!(spec.info.name, "test-pkg");
        assert!(!spec.info.version.is_empty());
    }

    // ─── generate_arg_types Tests ────────────────────────────────────

    fn gen_args(spec: &PackageSpec) -> String {
        let dir = tempfile::tempdir().unwrap();
        generate_arg_types(spec, dir.path().to_str().unwrap());
        std::fs::read_to_string(dir.path().join("args.rs")).unwrap()
    }

    #[test]
    fn generate_arg_types_single_command() {
        let spec = PackageSpec::new("echo", "0.1.0").command(
            CommandSpec::implicit_default().args(&[ArgSchemaInfo::string("text").optional()]),
        );
        let code = gen_args(&spec);
        assert!(code.contains("pub struct DefaultArgs"));
        assert!(code.contains("pub text: Option<String>"));
        assert!(code.contains("impl ::zacor_package::FromArgs for DefaultArgs"));
    }

    #[test]
    fn generate_arg_types_multi_command() {
        let spec = PackageSpec::new("date", "0.1.0")
            .command(CommandSpec::named("default").args(&[ArgSchemaInfo::bool("utc")]))
            .command(
                CommandSpec::named("add").args(&[ArgSchemaInfo::string("duration").required()]),
            )
            .command(CommandSpec::named("diff").args(&[
                ArgSchemaInfo::string("from").required(),
                ArgSchemaInfo::string("to").required(),
            ]));
        let code = gen_args(&spec);
        assert!(code.contains("pub struct DefaultArgs"));
        assert!(code.contains("pub struct AddArgs"));
        assert!(code.contains("pub struct DiffArgs"));
    }

    #[test]
    fn generate_arg_types_type_mapping_string() {
        let spec = PackageSpec::new("t", "0.1.0").command(
            CommandSpec::implicit_default().args(&[ArgSchemaInfo::string("name").required()]),
        );
        let code = gen_args(&spec);
        assert!(code.contains("pub name: String"));
    }

    #[test]
    fn generate_arg_types_type_mapping_number() {
        let spec = PackageSpec::new("t", "0.1.0").command(
            CommandSpec::implicit_default().args(&[ArgSchemaInfo::number("count").optional()]),
        );
        let code = gen_args(&spec);
        assert!(code.contains("pub count: Option<f64>"));
    }

    #[test]
    fn generate_arg_types_type_mapping_integer() {
        let spec = PackageSpec::new("t", "0.1.0").command(
            CommandSpec::implicit_default().args(&[ArgSchemaInfo::integer("lines").required()]),
        );
        let code = gen_args(&spec);
        assert!(code.contains("pub lines: i64"));
    }

    #[test]
    fn generate_arg_types_type_mapping_bool() {
        let spec = PackageSpec::new("t", "0.1.0")
            .command(CommandSpec::implicit_default().args(&[ArgSchemaInfo::bool("verbose")]));
        let code = gen_args(&spec);
        // Bools are always plain bool, never Option<bool>
        assert!(code.contains("pub verbose: bool"));
    }

    #[test]
    fn generate_arg_types_type_mapping_path() {
        let spec = PackageSpec::new("t", "0.1.0").command(
            CommandSpec::implicit_default().args(&[ArgSchemaInfo::path("file").required()]),
        );
        let code = gen_args(&spec);
        assert!(code.contains("pub file: PathBuf"));
    }

    #[test]
    fn generate_arg_types_optional_string_is_option() {
        let spec = PackageSpec::new("t", "0.1.0").command(
            CommandSpec::implicit_default().args(&[ArgSchemaInfo::string("format").optional()]),
        );
        let code = gen_args(&spec);
        assert!(code.contains("pub format: Option<String>"));
    }

    #[test]
    fn generate_arg_types_required_string_is_plain() {
        let spec = PackageSpec::new("t", "0.1.0").command(
            CommandSpec::implicit_default().args(&[ArgSchemaInfo::string("name").required()]),
        );
        let code = gen_args(&spec);
        assert!(code.contains("pub name: String"));
        assert!(!code.contains("pub name: Option<String>"));
    }

    #[test]
    fn generate_arg_types_string_with_default_is_plain() {
        let spec = PackageSpec::new("t", "0.1.0").command(
            CommandSpec::implicit_default()
                .args(&[ArgSchemaInfo::string("sep").default(DefaultValue::String(","))]),
        );
        let code = gen_args(&spec);
        assert!(code.contains("pub sep: String"));
        assert!(!code.contains("pub sep: Option<String>"));
    }

    #[test]
    fn generate_arg_types_kebab_case_command_name() {
        let spec =
            PackageSpec::new("t", "0.1.0").command(CommandSpec::named("some-name").args(&[]));
        let code = gen_args(&spec);
        assert!(code.contains("pub struct SomeNameArgs"));
    }

    #[test]
    fn generate_arg_types_idempotent_write() {
        let dir = tempfile::tempdir().unwrap();
        let spec = PackageSpec::new("echo", "0.1.0").command(
            CommandSpec::implicit_default().args(&[ArgSchemaInfo::string("text").optional()]),
        );

        // First write
        generate_arg_types(&spec, dir.path().to_str().unwrap());
        let path = dir.path().join("args.rs");
        let mtime1 = std::fs::metadata(&path).unwrap().modified().unwrap();

        // Small delay to ensure mtime would differ if rewritten
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Second write with same content
        generate_arg_types(&spec, dir.path().to_str().unwrap());
        let mtime2 = std::fs::metadata(&path).unwrap().modified().unwrap();

        assert_eq!(
            mtime1, mtime2,
            "file should not be rewritten when content is unchanged"
        );
    }

    #[test]
    fn generate_arg_types_rewrites_on_change() {
        let dir = tempfile::tempdir().unwrap();
        let spec1 = PackageSpec::new("echo", "0.1.0").command(
            CommandSpec::implicit_default().args(&[ArgSchemaInfo::string("text").optional()]),
        );
        generate_arg_types(&spec1, dir.path().to_str().unwrap());
        let content1 = std::fs::read_to_string(dir.path().join("args.rs")).unwrap();

        // Different spec
        let spec2 = PackageSpec::new("echo", "0.1.0").command(
            CommandSpec::implicit_default().args(&[ArgSchemaInfo::string("msg").required()]),
        );
        generate_arg_types(&spec2, dir.path().to_str().unwrap());
        let content2 = std::fs::read_to_string(dir.path().join("args.rs")).unwrap();

        assert_ne!(content1, content2);
        assert!(content2.contains("pub msg: String"));
    }
}
