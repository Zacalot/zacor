//! Shared execution primitives for package dispatch.
//!
//! Used by both local dispatch (dispatch.rs) and remote server (serve.rs)
//! to build env vars, substitute placeholders, and execute invoke templates.

use crate::config::{self, GlobalConfig};
use crate::error::*;
use crate::package_definition::{CommandDefinition, InvokeTemplate};
use crate::receipt::Receipt;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

// ─── Env Var Building ────────────────────────────────────────────────

/// Build env vars and placeholder map for package execution.
///
/// Resolves config through the full layering system:
/// flags > env > project per-pkg > project [zr] > receipt config > global per-pkg > global [zr] > package defaults
#[allow(clippy::too_many_arguments)]
pub fn build_env_vars(
    home: &Path,
    package_name: &str,
    command_path: &str,
    version: &str,
    flags: &BTreeMap<String, String>,
    command: &CommandDefinition,
    receipt: &Receipt,
    global_config: &GlobalConfig,
    definition_config: &BTreeMap<String, serde_yml::Value>,
    project_root: Option<&Path>,
    project_data: bool,
    project_config: Option<&GlobalConfig>,
    cwd: Option<&Path>,
) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let mut env_vars = BTreeMap::new();
    let mut placeholders = BTreeMap::new();

    // System vars
    env_vars.insert("ZR_PACKAGE".to_string(), package_name.to_string());
    env_vars.insert("ZR_COMMAND".to_string(), command_path.to_string());
    env_vars.insert("ZR_VERSION".to_string(), version.to_string());
    env_vars.insert("ZR_HOME".to_string(), home.to_string_lossy().into_owned());

    // Project vars
    if let Some(root) = project_root {
        env_vars.insert(
            "ZR_PROJECT".to_string(),
            root.to_string_lossy().into_owned(),
        );
    }
    if project_data {
        if let Some(root) = project_root.or(cwd) {
            let data_dir = crate::paths::project_data_dir(root, package_name);
            env_vars.insert(
                "ZR_DATA".to_string(),
                data_dir.to_string_lossy().into_owned(),
            );
        }
    }

    // Collect all declared keys with BTreeSet for natural deduplication
    let all_keys: BTreeSet<&str> = command
        .args
        .keys()
        .chain(definition_config.keys())
        .map(|s| s.as_str())
        .collect();

    for key in &all_keys {
        let value = config::resolve(
            key,
            package_name,
            flags,
            receipt,
            global_config,
            definition_config,
            project_config,
        );

        if let Some(v) = value {
            let env_name = config::config_env_var_name(package_name, key);
            env_vars.insert(env_name, v.clone());
            placeholders.insert(key.to_string(), v.clone());
            placeholders.insert(format!("config.{}", key), v);
        }
    }

    (env_vars, placeholders)
}

// ─── Invoke Template Execution ───────────────────────────────────────

/// Execute an invoke template with placeholder substitution.
pub fn exec_invoke(
    invoke: &InvokeTemplate,
    env_vars: &BTreeMap<String, String>,
    placeholders: &BTreeMap<String, String>,
) -> Result<i32> {
    let tokens = match invoke {
        InvokeTemplate::String(s) => {
            shlex::split(s).ok_or_else(|| anyhow!("failed to tokenize invoke template: {}", s))?
        }
        InvokeTemplate::Array(arr) => arr.clone(),
    };

    if tokens.is_empty() {
        bail!("invoke template is empty");
    }

    // Substitute placeholders and filter
    let mut argv: Vec<String> = Vec::new();
    for token in &tokens {
        if let Some(substituted) = substitute_token(token, placeholders) {
            argv.push(substituted);
        }
    }

    if argv.is_empty() {
        bail!("invoke template produced empty command after substitution");
    }

    let program = &argv[0];
    let args = &argv[1..];

    let status = Command::new(program)
        .args(args)
        .envs(env_vars)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .with_context(|| format!("failed to execute: {}", program))?;

    Ok(status.code().unwrap_or(1))
}

// ─── Token Substitution ─────────────────────────────────────────────

/// Substitute `{arg_name}` and `{config.key}` placeholders in a token.
/// Returns None if the token consists entirely of missing optional placeholders.
pub fn substitute_token(token: &str, placeholders: &BTreeMap<String, String>) -> Option<String> {
    let mut result = String::with_capacity(token.len());
    let mut has_placeholder = false;
    let mut all_missing = true;
    let mut pos = 0;

    while pos < token.len() {
        if let Some(open) = token[pos..].find('{') {
            let abs_open = pos + open;
            // Push literal text before the '{'
            if open > 0 {
                result.push_str(&token[pos..abs_open]);
                all_missing = false;
            }
            if let Some(close) = token[abs_open + 1..].find('}') {
                let key = &token[abs_open + 1..abs_open + 1 + close];
                has_placeholder = true;
                if let Some(value) = placeholders.get(key) {
                    all_missing = false;
                    result.push_str(value);
                }
                pos = abs_open + 1 + close + 1;
            } else {
                // Unmatched '{' — push it and move on
                result.push('{');
                pos = abs_open + 1;
            }
        } else {
            // No more '{' — push remaining text
            if !token[pos..].is_empty() {
                result.push_str(&token[pos..]);
                all_missing = false;
            }
            break;
        }
    }

    if has_placeholder && all_missing {
        return None;
    }

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_token_basic() {
        let mut placeholders = BTreeMap::new();
        placeholders.insert("file".to_string(), "/path/to/file".to_string());
        let result = substitute_token("{file}", &placeholders);
        assert_eq!(result, Some("/path/to/file".to_string()));
    }

    #[test]
    fn test_substitute_token_missing_optional() {
        let placeholders = BTreeMap::new();
        let result = substitute_token("{optional-arg}", &placeholders);
        assert_eq!(result, None); // Entire token omitted
    }

    #[test]
    fn test_substitute_token_mixed() {
        let mut placeholders = BTreeMap::new();
        placeholders.insert("format".to_string(), "mp3".to_string());
        let result = substitute_token("-f {format}", &placeholders);
        assert_eq!(result, Some("-f mp3".to_string()));
    }

    #[test]
    fn test_substitute_token_config_prefix() {
        let mut placeholders = BTreeMap::new();
        placeholders.insert("model".to_string(), "base".to_string());
        placeholders.insert("config.model".to_string(), "base".to_string());
        let result = substitute_token("{config.model}", &placeholders);
        assert_eq!(result, Some("base".to_string()));
    }

    fn make_receipt() -> crate::receipt::Receipt {
        crate::receipt::Receipt {
            schema: 1,
            current: "1.0.0".to_string(),
            active: true,
            mode: None,
            transport: None,
            config: BTreeMap::new(),
            versions: BTreeMap::new(),
        }
    }

    #[test]
    fn test_zr_data_set_without_project_root() {
        let home = Path::new("/home/user/.zr");
        let cwd = Path::new("/tmp/newproject");
        let receipt = make_receipt();
        let global_config = config::GlobalConfig::default();
        let command = crate::package_definition::CommandDefinition::default();
        let def_config = BTreeMap::new();

        let (env_vars, _) = build_env_vars(
            home,
            "wf",
            "init",
            "1.0.0",
            &BTreeMap::new(),
            &command,
            &receipt,
            &global_config,
            &def_config,
            None, // no project_root — no .zr/ found
            true, // project_data
            None,
            Some(cwd), // cwd fallback
        );

        // ZR_DATA should be set using cwd as fallback root
        let expected = format!("{}", cwd.join(".zr").join("wf").display());
        assert_eq!(env_vars.get("ZR_DATA").unwrap(), &expected);

        // ZR_PROJECT should NOT be set when no .zr/ exists
        assert!(!env_vars.contains_key("ZR_PROJECT"));
    }
}
