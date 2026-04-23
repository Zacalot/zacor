use std::collections::HashSet;
use std::path::{Path, PathBuf};

use regex::Regex;

pub const SKILL_ALLOWED_TAGS: &[&str] = &[
    "identity",
    "input",
    "context",
    "mode",
    "strategy",
    "constraints",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillPromptValidationError {
    pub prompt_file: String,
    pub kind: SkillPromptValidationKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillPromptValidationKind {
    DisallowedTag { tag: String },
    MissingRequiredTag { tag: String },
}

impl SkillPromptValidationError {
    pub fn message(&self) -> String {
        match &self.kind {
            SkillPromptValidationKind::DisallowedTag { tag } => {
                format!("disallowed tag '<{tag}>' in skill prompt body")
            }
            SkillPromptValidationKind::MissingRequiredTag { tag } => {
                format!("missing required tag '<{tag}>' in skill prompt body")
            }
        }
    }

    pub fn tag(&self) -> &str {
        match &self.kind {
            SkillPromptValidationKind::DisallowedTag { tag }
            | SkillPromptValidationKind::MissingRequiredTag { tag } => tag,
        }
    }
}

pub fn validate_skill_prompt_body(
    prompt_file: impl Into<String>,
    body: &str,
) -> Vec<SkillPromptValidationError> {
    let prompt_file = prompt_file.into();
    let body = strip_markdown_code(body);
    let tag_re = Regex::new(r"</?([a-zA-Z][a-zA-Z0-9-]*)>").unwrap();
    let open_tag_re = Regex::new(r"<([a-zA-Z][a-zA-Z0-9-]*)>").unwrap();
    let allowed: HashSet<&str> = SKILL_ALLOWED_TAGS.iter().copied().collect();

    let mut diagnostics = Vec::new();
    let mut seen_disallowed = HashSet::new();
    for captures in tag_re.captures_iter(&body) {
        let name = captures.get(1).unwrap().as_str();
        if !allowed.contains(name) && seen_disallowed.insert(name.to_string()) {
            diagnostics.push(SkillPromptValidationError {
                prompt_file: prompt_file.clone(),
                kind: SkillPromptValidationKind::DisallowedTag {
                    tag: name.to_string(),
                },
            });
        }
    }

    let seen_open: HashSet<String> = open_tag_re
        .captures_iter(&body)
        .map(|captures| captures.get(1).unwrap().as_str().to_string())
        .collect();

    for required in ["identity", "input"] {
        if !seen_open.contains(required) {
            diagnostics.push(SkillPromptValidationError {
                prompt_file: prompt_file.clone(),
                kind: SkillPromptValidationKind::MissingRequiredTag {
                    tag: required.to_string(),
                },
            });
        }
    }

    diagnostics
}

fn strip_markdown_code(body: &str) -> String {
    let fenced_code_re = Regex::new(r"(?s)```.*?```").unwrap();
    let without_fences = fenced_code_re.replace_all(body, "");
    let inline_code_re = Regex::new(r"`[^`]*`").unwrap();
    inline_code_re.replace_all(&without_fences, "").into_owned()
}

// ---------------------------------------------------------------------------
// Template structs (shared between build-time codegen and runtime)
// ---------------------------------------------------------------------------

/// A skill template embedded in a package binary.
#[derive(Debug, Clone)]
pub struct SkillTemplate {
    pub name: String,
    pub description: String,
    pub argument_hint: Option<String>,
    pub allowed_tools: Option<String>,
    pub effort: Option<String>,
    pub prompt: String,
}

/// An agent template embedded in a package binary.
#[derive(Debug, Clone)]
pub struct AgentTemplate {
    pub name: String,
    pub description: String,
    pub tools: Option<String>,
    pub model: Option<String>,
    pub prompt: String,
}

// ---------------------------------------------------------------------------
// Feature definitions
// ---------------------------------------------------------------------------

pub struct Feature {
    pub name: &'static str,
    pub dir: &'static str,
}

pub const FEATURES: &[Feature] = &[
    Feature {
        name: "claude-code",
        dir: ".claude",
    },
    Feature {
        name: "gemini",
        dir: ".gemini",
    },
    Feature {
        name: "opencode",
        dir: ".opencode",
    },
    Feature {
        name: "codex",
        dir: ".codex",
    },
];

pub fn find_feature(name: &str) -> Option<&'static Feature> {
    FEATURES.iter().find(|feature| feature.name == name)
}

/// Validate feature names against known set. Returns error with valid names on failure.
pub fn validate_features(names: &[String]) -> Result<(), String> {
    for name in names {
        if find_feature(name).is_none() {
            let valid: Vec<&str> = FEATURES.iter().map(|feature| feature.name).collect();
            return Err(format!(
                "Unknown feature '{}'. Valid features: {}",
                name,
                valid.join(", ")
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Auto-prefix naming
// ---------------------------------------------------------------------------

/// Compute the prefixed name: `zr-<pkg>-<local>`.
pub fn prefixed_name(pkg_name: &str, local_name: &str) -> String {
    format!("zr-{pkg_name}-{local_name}")
}

// ---------------------------------------------------------------------------
// Feature rendering — Skills
// ---------------------------------------------------------------------------

impl Feature {
    /// Render a skill shim file for this feature.
    pub fn render_skill(&self, template: &SkillTemplate, pkg_name: &str) -> String {
        let full_name = prefixed_name(pkg_name, &template.name);
        match self.name {
            "codex" => self.render_skill_codex(template, pkg_name, &full_name),
            "opencode" => self.render_skill_opencode(template, pkg_name, &full_name),
            _ => self.render_skill_default(template, pkg_name, &full_name),
        }
    }

    /// Default skill rendering (claude-code, gemini, opencode): markdown + YAML frontmatter + prompt.
    fn render_skill_default(
        &self,
        template: &SkillTemplate,
        pkg_name: &str,
        full_name: &str,
    ) -> String {
        let prompt = strip_comments(&template.prompt);
        let mut fm = format!(
            "---\nname: {full_name}\ndescription: {}\n",
            template.description,
        );
        if let Some(hint) = &template.argument_hint {
            fm.push_str(&format!("argument-hint: \"{hint}\"\n"));
        }
        if let Some(tools) = &template.allowed_tools {
            let mapped = self.map_tools(tools);
            if !mapped.is_empty() {
                fm.push_str(&format!("allowed-tools: {}\n", mapped.join(", ")));
            }
        }
        if let Some(effort) = &template.effort {
            fm.push_str(&format!("effort: {effort}\n"));
        }
        fm.push_str("---\n");
        let _ = pkg_name;
        fm.push_str(&prompt);
        if !prompt.ends_with('\n') {
            fm.push('\n');
        }
        fm
    }

    /// OpenCode skill rendering: only description in frontmatter (name comes from filename).
    fn render_skill_opencode(
        &self,
        template: &SkillTemplate,
        _pkg_name: &str,
        _full_name: &str,
    ) -> String {
        let prompt = strip_comments(&template.prompt);
        let mut out = format!("---\ndescription: {}\n---\n", template.description,);
        out.push_str(&prompt);
        if !prompt.ends_with('\n') {
            out.push('\n');
        }
        out
    }

    /// Codex skill rendering: TOML format.
    fn render_skill_codex(
        &self,
        template: &SkillTemplate,
        pkg_name: &str,
        full_name: &str,
    ) -> String {
        let prompt = strip_comments(&template.prompt);
        let sandbox = template
            .allowed_tools
            .as_deref()
            .map(Self::codex_sandbox_mode)
            .unwrap_or("read-only");

        let mut out = format!(
            "name = \"{full_name}\"\ndescription = \"{}\"\nsandbox_mode = \"{sandbox}\"\n",
            template.description,
        );
        out.push_str("developer_instructions = \"\"\"\n");
        let _ = pkg_name;
        out.push_str(&prompt);
        if !prompt.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("\"\"\"\n");
        out
    }

    pub fn skill_path(&self, root: &Path, prefixed: &str) -> PathBuf {
        match self.name {
            "opencode" => root
                .join(self.dir)
                .join("command")
                .join(format!("{prefixed}.md")),
            _ => root
                .join(self.dir)
                .join("skills")
                .join(prefixed)
                .join("SKILL.md"),
        }
    }

    // -----------------------------------------------------------------------
    // Feature rendering — Agents
    // -----------------------------------------------------------------------

    pub fn render_agent(&self, template: &AgentTemplate, pkg_name: &str) -> String {
        let full_name = prefixed_name(pkg_name, &template.name);
        match self.name {
            "codex" => self.render_agent_codex(template, &full_name),
            "opencode" => self.render_agent_opencode(template, &full_name),
            "gemini" => self.render_agent_gemini(template, &full_name),
            _ => self.render_agent_claude(template, &full_name),
        }
    }

    fn render_agent_claude(&self, template: &AgentTemplate, full_name: &str) -> String {
        let prompt = strip_comments(&template.prompt);
        let mut fm = format!(
            "---\nname: {full_name}\ndescription: {}\n",
            template.description,
        );
        if let Some(tools_csv) = &template.tools {
            let mapped = self.map_tools(tools_csv);
            if !mapped.is_empty() {
                fm.push_str(&format!("tools: {}\n", mapped.join(", ")));
            }
        }
        if let Some(tier) = &template.model {
            if let Some(model_id) = self.map_model(tier) {
                fm.push_str(&format!("model: {model_id}\n"));
            }
        }
        fm.push_str("---\n");
        fm.push_str(&prompt);
        if !prompt.ends_with('\n') {
            fm.push('\n');
        }
        fm
    }

    fn render_agent_opencode(&self, template: &AgentTemplate, _full_name: &str) -> String {
        let prompt = strip_comments(&template.prompt);
        let mut fm = format!(
            "---\ndescription: {}\nmode: subagent\n",
            template.description,
        );
        if let Some(tools_csv) = &template.tools {
            let mapped: Vec<String> = tools_csv
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .filter_map(|semantic| self.map_tool(semantic).map(String::from))
                .collect();
            if !mapped.is_empty() {
                fm.push_str("permission:\n");
                for tool in &mapped {
                    fm.push_str(&format!("  {tool}: allow\n"));
                }
            }
        }
        if let Some(tier) = &template.model {
            if let Some(model_id) = self.map_model(tier) {
                fm.push_str(&format!("model: {model_id}\n"));
            }
        }
        fm.push_str("---\n");
        fm.push_str(&prompt);
        if !prompt.ends_with('\n') {
            fm.push('\n');
        }
        fm
    }

    fn render_agent_gemini(&self, template: &AgentTemplate, full_name: &str) -> String {
        let prompt = strip_comments(&template.prompt);
        let mut fm = format!(
            "---\nname: {full_name}\ndescription: {}\n",
            template.description,
        );
        if let Some(tools_csv) = &template.tools {
            let mapped = self.map_tools(tools_csv);
            if !mapped.is_empty() {
                fm.push_str("tools:\n");
                for tool in &mapped {
                    fm.push_str(&format!("  - {tool}\n"));
                }
            }
        }
        if let Some(tier) = &template.model {
            if let Some(model_id) = self.map_model(tier) {
                fm.push_str(&format!("model: {model_id}\n"));
            }
        }
        fm.push_str("---\n");
        fm.push_str(&prompt);
        if !prompt.ends_with('\n') {
            fm.push('\n');
        }
        fm
    }

    fn render_agent_codex(&self, template: &AgentTemplate, full_name: &str) -> String {
        let prompt = strip_comments(&template.prompt);
        let sandbox = template
            .tools
            .as_deref()
            .map(Self::codex_sandbox_mode)
            .unwrap_or("read-only");

        let mut out = format!(
            "name = \"{full_name}\"\ndescription = \"{}\"\nsandbox_mode = \"{sandbox}\"\n",
            template.description,
        );
        out.push_str("developer_instructions = \"\"\"\n");
        out.push_str(&prompt);
        if !prompt.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("\"\"\"\n");
        out
    }

    pub fn agent_path(&self, root: &Path, prefixed: &str) -> PathBuf {
        let ext = if self.name == "codex" { "toml" } else { "md" };
        root.join(self.dir)
            .join("agents")
            .join(format!("{prefixed}.{ext}"))
    }

    // -----------------------------------------------------------------------
    // Tool & model mapping
    // -----------------------------------------------------------------------

    pub fn map_tool(&self, semantic: &str) -> Option<&'static str> {
        let semantic = semantic.to_lowercase();
        match (self.name, semantic.as_str()) {
            ("claude-code", "web-search") => Some("WebSearch"),
            ("claude-code", "web-fetch") => Some("WebFetch"),
            ("claude-code", "read") => Some("Read"),
            ("claude-code", "edit") => Some("Edit"),
            ("claude-code", "shell") => Some("Bash"),
            ("claude-code", "grep") => Some("Grep"),
            ("claude-code", "glob") => Some("Glob"),
            ("claude-code", "write") => Some("Write"),
            ("claude-code", "ask") => Some("AskUserQuestion"),

            ("opencode", "web-search") => Some("websearch"),
            ("opencode", "web-fetch") => Some("webfetch"),
            ("opencode", "read") => None,
            ("opencode", "edit") => Some("edit"),
            ("opencode", "shell") => Some("bash"),
            ("opencode", "grep") => Some("grep"),
            ("opencode", "glob") => Some("glob"),
            ("opencode", "write") => Some("write"),
            ("opencode", "ask") => Some("question"),

            ("gemini", "web-search") => Some("google_web_search"),
            ("gemini", "web-fetch") => Some("web_fetch"),
            ("gemini", "read") => Some("read_file"),
            ("gemini", "edit") => Some("replace"),
            ("gemini", "shell") => Some("run_shell_command"),
            ("gemini", "grep") => Some("grep_search"),
            ("gemini", "glob") => Some("glob"),
            ("gemini", "write") => Some("write_file"),
            ("gemini", "ask") => None,

            ("codex", _) => None,
            _ => None,
        }
    }

    pub fn map_tools(&self, tools_csv: &str) -> Vec<String> {
        tools_csv
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .filter_map(|semantic| {
                // Handle agent(name) pattern
                if let Some(name) = semantic
                    .strip_prefix("agent(")
                    .and_then(|s| s.strip_suffix(')'))
                {
                    return match self.name {
                        "claude-code" => Some(format!("Agent({name})")),
                        _ => None,
                    };
                }
                self.map_tool(semantic).map(String::from).or_else(|| {
                    if self.name == "codex" {
                        None
                    } else {
                        Some(semantic.to_string())
                    }
                })
            })
            .collect()
    }

    pub fn map_model(&self, tier: &str) -> Option<&'static str> {
        match (self.name, tier) {
            ("claude-code", "fast") => Some("haiku"),
            ("claude-code", "default") => None,
            ("claude-code", "capable") => Some("sonnet"),

            ("opencode", "fast") => Some("anthropic/claude-haiku-4-5"),
            ("opencode", "default") => None,
            ("opencode", "capable") => Some("anthropic/claude-sonnet-4-6"),

            ("gemini", "fast") => Some("gemini-2.5-flash"),
            ("gemini", "default") => None,
            ("gemini", "capable") => Some("gemini-2.5-pro"),

            ("codex", _) => None,
            _ => None,
        }
    }

    fn codex_sandbox_mode(tools_csv: &str) -> &'static str {
        let write_tools = ["edit", "shell"];
        let has_write = tools_csv
            .split(',')
            .map(|s| s.trim())
            .any(|t| write_tools.contains(&t));
        if has_write {
            "workspace-write"
        } else {
            "read-only"
        }
    }
}

// ---------------------------------------------------------------------------
// Generation orchestrator
// ---------------------------------------------------------------------------

/// Generate all skill and agent files for the given features.
pub fn generate(
    root: &Path,
    features: &[String],
    pkg_name: &str,
    skills: &[SkillTemplate],
    agents: &[AgentTemplate],
) -> Result<(), String> {
    for feature_name in features {
        let feature =
            find_feature(feature_name).ok_or_else(|| format!("Unknown feature: {feature_name}"))?;

        for template in skills {
            let full = prefixed_name(pkg_name, &template.name);
            let content = feature.render_skill(template, pkg_name);
            let path = feature.skill_path(root, &full);
            write_file(&path, &content)?;
        }

        for template in agents {
            let full = prefixed_name(pkg_name, &template.name);
            let content = feature.render_agent(template, pkg_name);
            let path = feature.agent_path(root, &full);
            write_file(&path, &content)?;
        }
    }
    Ok(())
}

fn write_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        crate::io::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    crate::io::fs::write(path, content.as_bytes())
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Runtime handlers
// ---------------------------------------------------------------------------

/// Resolve cross-package `{{pkg.name}}` template markers in a prompt string
/// by calling `zr <pkg> template <name>`.
#[cfg(not(target_family = "wasm"))]
pub fn resolve_cross_package_templates(prompt: &str) -> Result<String, String> {
    let re = regex::Regex::new(r"\{\{([a-zA-Z0-9_-]+)\.([a-zA-Z0-9_-]+)\}\}").unwrap();
    let mut result = prompt.to_string();
    // Collect matches first to avoid borrow issues
    let matches: Vec<(String, String, String)> = re
        .captures_iter(prompt)
        .map(|cap| (cap[0].to_string(), cap[1].to_string(), cap[2].to_string()))
        .collect();

    for (full_match, pkg, name) in matches {
        let output = std::process::Command::new("zr")
            .args(["--text", &pkg, "template", &name])
            .output()
            .map_err(|e| format!("Failed to call zr {pkg} template {name}: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "Cross-package template {{{{{pkg}.{name}}}}} failed: {stderr}"
            ));
        }

        let content = String::from_utf8_lossy(&output.stdout).to_string();
        result = result.replace(&full_match, content.trim_end());
    }
    Ok(result)
}

/// Wasm stub: cross-package template resolution requires subprocess dispatch,
/// which is unavailable under WASI preview 1. A future host capability could
/// provide this; until then, callers must resolve cross-package markers
/// host-side before the wasm guest sees them.
#[cfg(target_family = "wasm")]
pub fn resolve_cross_package_templates(prompt: &str) -> Result<String, String> {
    // Markers containing `.` are left verbatim; if any remain at runtime,
    // flag the unavailability.
    if prompt.contains("{{") && regex::Regex::new(r"\{\{[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+\}\}")
        .unwrap()
        .is_match(prompt)
    {
        return Err(
            "cross-package template resolution is unavailable under wasm (no subprocess support); \
             host should resolve markers before dispatch"
                .to_string(),
        );
    }
    Ok(prompt.to_string())
}

/// Look up a skill template by local name.
pub fn find_skill<'a>(name: &str, templates: &'a [SkillTemplate]) -> Option<&'a SkillTemplate> {
    templates.iter().find(|t| t.name == name)
}

/// Handle the `skill` command: look up, substitute arguments, resolve preprocessors.
pub fn handle_skill(
    name: &str,
    args: Option<&str>,
    templates: &[SkillTemplate],
) -> Result<String, String> {
    let template = find_skill(name, templates).ok_or_else(|| format!("Unknown skill '{name}'"))?;

    let mut body = template.prompt.clone();
    body = substitute_arguments(&body, args.unwrap_or(""));
    body = resolve_preprocessor(&body);
    body = strip_comments(&body);
    Ok(body)
}

/// Handle the `template` command: serve by name or list all.
pub fn handle_template(
    name: &str,
    list: bool,
    templates: &[(&str, &str)],
) -> Result<String, String> {
    if list {
        let mut out = String::new();
        for (n, _) in templates {
            out.push_str(n);
            out.push('\n');
        }
        return Ok(out);
    }

    templates
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, content)| content.to_string())
        .ok_or_else(|| format!("Unknown template '{name}'"))
}

fn strip_comments(body: &str) -> String {
    let re = regex::Regex::new(r"(?s)<!--.*?-->[ \t]*\n?").unwrap();
    re.replace_all(body, "").into_owned()
}

fn substitute_arguments(body: &str, args: &str) -> String {
    body.replace("$ARGUMENTS", args)
}

fn resolve_preprocessor(body: &str) -> String {
    let re = regex::Regex::new(r"!`([^`]+)`").unwrap();
    re.replace_all(body, |caps: &regex::Captures| {
        let cmd = &caps[1];
        execute_shell(cmd)
    })
    .into_owned()
}

#[cfg(not(target_family = "wasm"))]
fn execute_shell(cmd: &str) -> String {
    let shell_result = if cfg!(windows) {
        std::process::Command::new("cmd").args(["/C", cmd]).output()
    } else {
        std::process::Command::new("sh").args(["-c", cmd]).output()
    };

    match shell_result {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string(),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            format!("ERROR: {cmd}: {stderr}")
        }
        Err(e) => format!("ERROR: {cmd}: {e}"),
    }
}

/// Wasm stub: preprocessor `!`cmd`` requires subprocess execution, unavailable
/// under WASI preview 1. Leaves the marker in place with an error note.
#[cfg(target_family = "wasm")]
fn execute_shell(cmd: &str) -> String {
    format!("ERROR: {cmd}: shell preprocessor unavailable under wasm")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_known_features() {
        assert!(find_feature("claude-code").is_some());
        assert!(find_feature("gemini").is_some());
        assert!(find_feature("opencode").is_some());
        assert!(find_feature("codex").is_some());
    }

    #[test]
    fn find_unknown_returns_none() {
        assert!(find_feature("vscode").is_none());
    }

    #[test]
    fn validate_features_ok() {
        validate_features(&["claude-code".into(), "gemini".into()]).unwrap();
    }

    #[test]
    fn validate_features_err() {
        let err = validate_features(&["claude-code".into(), "vscode".into()]).unwrap_err();
        assert!(err.contains("vscode"));
        assert!(err.contains("Valid features"));
    }

    #[test]
    fn test_prefixed_name() {
        assert_eq!(prefixed_name("wf", "search-web"), "zr-wf-search-web");
        assert_eq!(prefixed_name("ops", "deploy"), "zr-ops-deploy");
    }

    // --- Skill rendering ---

    fn test_skill() -> SkillTemplate {
        SkillTemplate {
            name: "search-web".into(),
            description: "Broad web search".into(),
            argument_hint: Some("<topic>".into()),
            allowed_tools: Some("web-search, read, shell".into()),
            effort: None,
            prompt: "Search prompt body".into(),
        }
    }

    #[test]
    fn render_skill_claude_code() {
        let p = find_feature("claude-code").unwrap();
        let out = p.render_skill(&test_skill(), "wf");
        assert!(out.contains("name: zr-wf-search-web"));
        assert!(out.contains("description: Broad web search"));
        assert!(!out.contains("disable-model-invocation: true"));
        assert!(out.contains("allowed-tools: WebSearch, Read, Bash"));
        assert!(out.contains("argument-hint: \"<topic>\""));
        assert!(out.contains("Search prompt body"));
        assert!(!out.contains("!`zr --format text wf skill search-web $ARGUMENTS`"));
    }

    #[test]
    fn render_skill_opencode() {
        let p = find_feature("opencode").unwrap();
        let out = p.render_skill(&test_skill(), "wf");
        assert!(!out.contains("name:"));
        assert!(out.contains("description: Broad web search"));
        assert!(!out.contains("allowed-tools:"));
        assert!(!out.contains("argument-hint:"));
        assert!(!out.contains("effort:"));
        assert!(out.contains("Search prompt body"));
    }

    #[test]
    fn render_skill_adds_bash_if_missing() {
        let p = find_feature("claude-code").unwrap();
        let mut skill = test_skill();
        skill.allowed_tools = Some("read".into());
        let out = p.render_skill(&skill, "wf");
        assert!(out.contains("allowed-tools: Read"));
        assert!(!out.contains("allowed-tools: Read, Bash"));
    }

    #[test]
    fn render_skill_no_tools_defaults_to_bash() {
        let p = find_feature("claude-code").unwrap();
        let mut skill = test_skill();
        skill.allowed_tools = None;
        let out = p.render_skill(&skill, "wf");
        assert!(!out.contains("allowed-tools:"));
    }

    #[test]
    fn render_skill_codex() {
        let p = find_feature("codex").unwrap();
        let out = p.render_skill(&test_skill(), "wf");
        assert!(out.contains("name = \"zr-wf-search-web\""));
        assert!(out.contains("sandbox_mode ="));
        assert!(out.contains("developer_instructions = \"\"\""));
        assert!(out.contains("Search prompt body"));
        assert!(!out.contains("!`zr --format text wf skill search-web $ARGUMENTS`"));
    }

    #[test]
    fn render_skill_strips_comments() {
        let p = find_feature("claude-code").unwrap();
        let mut skill = test_skill();
        skill.prompt = "before\n<!-- internal note -->\nafter".into();
        let out = p.render_skill(&skill, "wf");
        assert!(out.contains("before\nafter"));
        assert!(!out.contains("<!-- internal note -->"));
    }

    #[test]
    fn skill_path_uses_prefix() {
        let p = find_feature("claude-code").unwrap();
        let path = p.skill_path(Path::new("/project"), "zr-wf-search-web");
        let s = crate::path_str(&path);
        assert!(s.contains(".claude/skills/zr-wf-search-web/SKILL.md"));
    }

    #[test]
    fn skill_path_opencode_uses_command_dir() {
        let p = find_feature("opencode").unwrap();
        let path = p.skill_path(Path::new("/project"), "zr-wf-search-web");
        let s = crate::path_str(&path);
        assert!(s.contains(".opencode/command/zr-wf-search-web.md"));
    }

    // --- Agent rendering ---

    fn test_agent() -> AgentTemplate {
        AgentTemplate {
            name: "researcher".into(),
            description: "Web research subagent".into(),
            tools: Some("web-search, web-fetch".into()),
            model: Some("fast".into()),
            prompt: "You are a web research agent.\n".into(),
        }
    }

    #[test]
    fn render_agent_claude_code() {
        let p = find_feature("claude-code").unwrap();
        let out = p.render_agent(&test_agent(), "wf");
        assert!(out.contains("name: zr-wf-researcher"));
        assert!(out.contains("tools: WebSearch, WebFetch"));
        assert!(out.contains("model: haiku"));
        assert!(out.contains("You are a web research agent."));
    }

    #[test]
    fn render_agent_opencode() {
        let p = find_feature("opencode").unwrap();
        let out = p.render_agent(&test_agent(), "wf");
        assert!(out.contains("mode: subagent"));
        assert!(out.contains("websearch: allow"));
        assert!(out.contains("webfetch: allow"));
    }

    #[test]
    fn render_agent_gemini() {
        let p = find_feature("gemini").unwrap();
        let out = p.render_agent(&test_agent(), "wf");
        assert!(out.contains("name: zr-wf-researcher"));
        assert!(out.contains("- google_web_search"));
        assert!(out.contains("- web_fetch"));
        assert!(out.contains("model: gemini-2.5-flash"));
    }

    #[test]
    fn render_agent_codex() {
        let p = find_feature("codex").unwrap();
        let out = p.render_agent(&test_agent(), "wf");
        assert!(out.contains("name = \"zr-wf-researcher\""));
        assert!(out.contains("sandbox_mode = \"read-only\""));
    }

    // --- Tool mapping ---

    #[test]
    fn map_tool_claude_code() {
        let p = find_feature("claude-code").unwrap();
        assert_eq!(p.map_tool("web-search"), Some("WebSearch"));
        assert_eq!(p.map_tool("shell"), Some("Bash"));
    }

    #[test]
    fn map_tool_codex_returns_none() {
        let p = find_feature("codex").unwrap();
        assert_eq!(p.map_tool("web-search"), None);
    }

    #[test]
    fn map_tool_write() {
        assert_eq!(
            find_feature("claude-code").unwrap().map_tool("write"),
            Some("Write")
        );
        assert_eq!(
            find_feature("opencode").unwrap().map_tool("write"),
            Some("write")
        );
        assert_eq!(
            find_feature("gemini").unwrap().map_tool("write"),
            Some("write_file")
        );
    }

    #[test]
    fn map_tool_ask() {
        assert_eq!(
            find_feature("claude-code").unwrap().map_tool("ask"),
            Some("AskUserQuestion")
        );
        assert_eq!(
            find_feature("opencode").unwrap().map_tool("ask"),
            Some("question")
        );
        assert_eq!(find_feature("gemini").unwrap().map_tool("ask"), None);
    }

    #[test]
    fn map_tools_agent_pattern() {
        let cc = find_feature("claude-code").unwrap();
        let mapped = cc.map_tools("agent(zr-wf-researcher), web-search");
        assert_eq!(mapped, vec!["Agent(zr-wf-researcher)", "WebSearch"]);

        let oc = find_feature("opencode").unwrap();
        let mapped = oc.map_tools("agent(zr-wf-researcher), web-search");
        assert_eq!(mapped, vec!["websearch"]);

        let gem = find_feature("gemini").unwrap();
        let mapped = gem.map_tools("agent(zr-wf-researcher), web-search");
        assert_eq!(mapped, vec!["google_web_search"]);

        let cdx = find_feature("codex").unwrap();
        let mapped = cdx.map_tools("agent(zr-wf-researcher), web-search");
        assert!(mapped.is_empty());
    }

    #[test]
    fn map_tools_passthrough() {
        let p = find_feature("claude-code").unwrap();
        let mapped = p.map_tools("web-search, CustomTool");
        assert_eq!(mapped, vec!["WebSearch", "CustomTool"]);
    }

    // --- Model mapping ---

    #[test]
    fn map_model_fast() {
        assert_eq!(
            find_feature("claude-code").unwrap().map_model("fast"),
            Some("haiku")
        );
        assert_eq!(
            find_feature("gemini").unwrap().map_model("fast"),
            Some("gemini-2.5-flash")
        );
    }

    #[test]
    fn map_model_default_inherits() {
        assert_eq!(
            find_feature("claude-code").unwrap().map_model("default"),
            None
        );
    }

    // --- Generation orchestrator ---

    #[test]
    fn generate_creates_files() {
        let tmp = tempfile::tempdir().unwrap();
        let skills = vec![test_skill()];
        let agents = vec![test_agent()];

        generate(tmp.path(), &["claude-code".into()], "wf", &skills, &agents).unwrap();

        let skill_path = tmp.path().join(".claude/skills/zr-wf-search-web/SKILL.md");
        assert!(skill_path.exists());
        let content = std::fs::read_to_string(&skill_path).unwrap();
        assert!(content.contains("name: zr-wf-search-web"));

        let agent_path = tmp.path().join(".claude/agents/zr-wf-researcher.md");
        assert!(agent_path.exists());
        let content = std::fs::read_to_string(&agent_path).unwrap();
        assert!(content.contains("name: zr-wf-researcher"));
    }

    #[test]
    fn generate_multi_feature() {
        let tmp = tempfile::tempdir().unwrap();
        let skills = vec![test_skill()];

        generate(
            tmp.path(),
            &["claude-code".into(), "gemini".into()],
            "wf",
            &skills,
            &[],
        )
        .unwrap();

        assert!(tmp
            .path()
            .join(".claude/skills/zr-wf-search-web/SKILL.md")
            .exists());
        assert!(tmp
            .path()
            .join(".gemini/skills/zr-wf-search-web/SKILL.md")
            .exists());
    }

    // --- Runtime handlers ---

    #[test]
    fn handle_skill_substitutes_args() {
        let templates = vec![SkillTemplate {
            name: "test".into(),
            description: "".into(),
            argument_hint: None,
            allowed_tools: None,
            effort: None,
            prompt: "Process $ARGUMENTS now.".into(),
        }];
        let result = handle_skill("test", Some("my-input"), &templates).unwrap();
        assert_eq!(result, "Process my-input now.");
    }

    #[test]
    fn handle_skill_unknown() {
        let result = handle_skill("nonexistent", None, &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown skill"));
    }

    #[test]
    fn handle_skill_strips_comments() {
        let templates = vec![SkillTemplate {
            name: "test".into(),
            description: "".into(),
            argument_hint: None,
            allowed_tools: None,
            effort: None,
            prompt: "before\n<!-- TODO: fix this -->\nafter".into(),
        }];
        let result = handle_skill("test", None, &templates).unwrap();
        assert_eq!(result, "before\nafter");
    }

    #[test]
    fn handle_skill_strips_multiline_comments() {
        let templates = vec![SkillTemplate {
            name: "test".into(),
            description: "".into(),
            argument_hint: None,
            allowed_tools: None,
            effort: None,
            prompt: "before\n<!-- line 1\nline 2 -->\nafter".into(),
        }];
        let result = handle_skill("test", None, &templates).unwrap();
        assert_eq!(result, "before\nafter");
    }

    #[test]
    fn handle_skill_preprocessor() {
        let templates = vec![SkillTemplate {
            name: "test".into(),
            description: "".into(),
            argument_hint: None,
            allowed_tools: None,
            effort: None,
            prompt: "before !`echo hello` after".into(),
        }];
        let result = handle_skill("test", None, &templates).unwrap();
        assert_eq!(result, "before hello after");
    }

    #[test]
    fn handle_template_by_name() {
        let templates = vec![("greeting", "Hello world")];
        let result = handle_template("greeting", false, &templates).unwrap();
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn handle_template_list() {
        let templates = vec![("alpha", "A"), ("beta", "B")];
        let result = handle_template("", true, &templates).unwrap();
        assert_eq!(result, "alpha\nbeta\n");
    }

    #[test]
    fn handle_template_unknown() {
        let result = handle_template("nonexistent", false, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn validate_skill_prompt_body_accepts_allowed_tags() {
        let diags = validate_skill_prompt_body(
            "test.md",
            "<identity>Test</identity>\n<input>$ARGUMENTS</input>\n<mode>Read</mode>",
        );
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn validate_skill_prompt_body_reports_disallowed_tags_once() {
        let diags = validate_skill_prompt_body(
            "test.md",
            "<identity>Test</identity>\n<input>$ARGUMENTS</input>\n<topic>a</topic>\n<topic>b</topic>",
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].prompt_file, "test.md");
        assert_eq!(diags[0].tag(), "topic");
        assert_eq!(
            diags[0].message(),
            "disallowed tag '<topic>' in skill prompt body"
        );
    }

    #[test]
    fn validate_skill_prompt_body_reports_missing_required_tags() {
        let diags = validate_skill_prompt_body("test.md", "<strategy>Only strategy</strategy>");
        assert_eq!(diags.len(), 2);
        assert!(diags.iter().any(|diag| diag.tag() == "identity"));
        assert!(diags.iter().any(|diag| diag.tag() == "input"));
    }

    #[test]
    fn validate_skill_prompt_body_ignores_cross_package_markers() {
        let diags = validate_skill_prompt_body(
            "test.md",
            "<identity>Test</identity>\n<input>$ARGUMENTS</input>\n{{otherpkg.shared}}",
        );
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn validate_skill_prompt_body_ignores_angle_brackets_inside_code() {
        let diags = validate_skill_prompt_body(
            "test.md",
            "<identity>Test</identity>\n<input>$ARGUMENTS</input>\nRun `zr wf archive <name>`.",
        );
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    }
}
