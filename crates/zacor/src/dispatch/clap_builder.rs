use crate::config;
use crate::error::*;
use crate::package_definition::{
    ArgType, ArgumentDefinition, CommandDefinition, PackageDefinition,
};
use clap::ArgAction;
use std::collections::BTreeMap;

/// Build a `clap::Command` from a `PackageDefinition`, mapping commands,
/// args, and the `default` command convention to clap's builder API.
pub fn build_clap_command(def: &PackageDefinition) -> clap::Command {
    let mut cmd = clap::Command::new(def.name.clone())
        .version(def.version.clone())
        .disable_help_subcommand(true);

    if let Some(ref desc) = def.description {
        cmd = cmd.about(desc.clone());
    }

    let has_default = def.commands.contains_key("default");
    let named: Vec<(&String, &CommandDefinition)> = def
        .commands
        .iter()
        .filter(|(k, _)| k.as_str() != "default")
        .collect();
    let has_named = !named.is_empty();

    match (has_default, has_named) {
        (true, false) => {
            // Single default: hoist args to root, no subcommand layer
            let default_cmd = &def.commands["default"];
            if def.description.is_none()
                && let Some(ref desc) = default_cmd.description
            {
                cmd = cmd.about(desc.clone());
            }
            let has_rest = default_cmd.args.values().any(|a| a.rest);
            for (name, arg_def) in &default_cmd.args {
                cmd = cmd.arg(build_arg(name, arg_def));
            }
            if has_rest {
                cmd = cmd.trailing_var_arg(true);
            }
        }
        (true, true) => {
            // Default + named: hoist default args, named become subcommands
            let default_cmd = &def.commands["default"];
            let has_rest = default_cmd.args.values().any(|a| a.rest);
            for (name, arg_def) in &default_cmd.args {
                cmd = cmd.arg(build_arg(name, arg_def));
            }
            if has_rest {
                cmd = cmd.trailing_var_arg(true);
            }
            cmd = cmd.subcommand_required(false);
            for (name, cmd_def) in &named {
                cmd = cmd.subcommand(build_subcommand(name, cmd_def));
            }
        }
        (false, _) => {
            // Named only: subcommand required
            cmd = cmd.subcommand_required(true);
            for (name, cmd_def) in &named {
                cmd = cmd.subcommand(build_subcommand(name, cmd_def));
            }
        }
    }

    cmd
}

/// Build a clap subcommand from a `CommandDefinition`, recursively
/// mapping nested commands.
fn build_subcommand(name: &str, def: &CommandDefinition) -> clap::Command {
    let mut cmd = clap::Command::new(name.to_string());

    if let Some(ref desc) = def.description {
        cmd = cmd.about(desc.clone());
    }

    let has_rest = def.args.values().any(|a| a.rest);
    for (arg_name, arg_def) in &def.args {
        cmd = cmd.arg(build_arg(arg_name, arg_def));
    }
    if has_rest {
        cmd = cmd.trailing_var_arg(true);
    }

    for (sub_name, sub_def) in &def.commands {
        cmd = cmd.subcommand(build_subcommand(sub_name, sub_def));
    }

    if !def.commands.is_empty() {
        cmd = cmd.subcommand_required(true);
    }

    cmd
}

/// Build a `clap::Arg` from an `ArgumentDefinition`, mapping ArgType
/// to clap value parsers and handling flag vs positional.
fn build_arg(name: &str, def: &ArgumentDefinition) -> clap::Arg {
    let mut arg = clap::Arg::new(name.to_string());

    // Flag vs positional — bools always become --flags
    if let Some(ref flag) = def.flag {
        arg = arg.long(flag.clone());
    } else if def.arg_type == ArgType::Bool {
        arg = arg.long(name.to_string());
    }

    // Type mapping
    match def.arg_type {
        ArgType::Bool => {
            arg = arg.action(ArgAction::SetTrue);
        }
        ArgType::Number | ArgType::Integer => {
            arg = arg.value_parser(parse_number);
        }
        ArgType::Path => {
            arg = arg.value_hint(clap::ValueHint::AnyPath);
        }
        ArgType::Choice => {
            if let Some(ref values) = def.values {
                arg = arg.value_parser(clap::builder::PossibleValuesParser::new(values.clone()));
            }
        }
        ArgType::String => {}
    }

    // Rest arg: consume all remaining tokens
    if def.rest {
        arg = arg.num_args(0..);
    }

    // Required (only non-Bool args without defaults)
    if def.arg_type != ArgType::Bool && def.required && def.default.is_none() {
        arg = arg.required(true);
    }

    // Default value (for required args with defaults — inserted at flag priority)
    if def.required
        && let Some(ref default) = def.default
    {
        arg = arg.default_value(config::yaml_value_to_string(default));
    }

    arg
}

fn parse_number(s: &str) -> std::result::Result<String, String> {
    s.parse::<f64>()
        .map_err(|_| format!("'{}' is not a valid number", s))?;
    Ok(s.to_string())
}

/// Parse CLI args using a clap Command built from a PackageDefinition.
/// Returns (command_path, parsed_flags) where command_path is like
/// "default", "transcribe", or "transcribe.batch".
#[allow(dead_code)]
pub(super) fn clap_parse(
    cmd: clap::Command,
    pkg_name: &str,
    args: &[String],
    def: &PackageDefinition,
) -> std::result::Result<(String, BTreeMap<String, String>), clap::Error> {
    let mut full_args = vec![pkg_name.to_string()];
    full_args.extend_from_slice(args);

    let matches = cmd.try_get_matches_from(full_args)?;

    // Check for subcommand match
    if let Some((sub_name, sub_matches)) = matches.subcommand()
        && let Some(cmd_def) = def.commands.get(sub_name)
    {
        let (sub_path, flags) = extract_from_command(sub_matches, cmd_def);
        let path = if sub_path.is_empty() {
            sub_name.to_string()
        } else {
            format!("{}.{}", sub_name, sub_path)
        };
        return Ok((path, flags));
    }

    // No subcommand matched — use default command
    if let Some(default_cmd) = def.commands.get("default") {
        let flags = extract_args(&matches, &default_cmd.args);
        return Ok(("default".to_string(), flags));
    }

    // Should not reach here (clap would have errored for named-only)
    Ok(("default".to_string(), BTreeMap::new()))
}

/// Recursively extract the deepest matched subcommand and its args.
#[allow(dead_code)]
fn extract_from_command(
    matches: &clap::ArgMatches,
    cmd_def: &CommandDefinition,
) -> (String, BTreeMap<String, String>) {
    if let Some((sub_name, sub_matches)) = matches.subcommand()
        && let Some(sub_cmd_def) = cmd_def.commands.get(sub_name)
    {
        let (sub_path, flags) = extract_from_command(sub_matches, sub_cmd_def);
        let path = if sub_path.is_empty() {
            sub_name.to_string()
        } else {
            format!("{}.{}", sub_name, sub_path)
        };
        return (path, flags);
    }

    let flags = extract_args(matches, &cmd_def.args);
    (String::new(), flags)
}

/// Extract arg values from clap matches using the argument definitions.
#[allow(dead_code)]
fn extract_args(
    matches: &clap::ArgMatches,
    arg_defs: &BTreeMap<String, ArgumentDefinition>,
) -> BTreeMap<String, String> {
    let mut flags = BTreeMap::new();
    for (name, def) in arg_defs {
        if def.arg_type == ArgType::Bool {
            if matches.get_flag(name) {
                flags.insert(name.clone(), "true".to_string());
            }
        } else if def.rest {
            if let Some(vals) = matches.get_many::<String>(name) {
                let joined: String = vals.cloned().collect::<Vec<_>>().join(" ");
                if !joined.is_empty() {
                    flags.insert(name.clone(), joined);
                }
            }
        } else if let Some(val) = matches.get_one::<String>(name) {
            flags.insert(name.clone(), val.clone());
        }
    }
    flags
}

/// Look up a CommandDefinition by dot-separated path (e.g., "transcribe.batch").
#[allow(dead_code)]
pub(super) fn find_command<'a>(
    commands: &'a BTreeMap<String, CommandDefinition>,
    path: &str,
) -> Result<&'a CommandDefinition> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = commands;
    let mut cmd = None;
    for part in &parts {
        match current.get(*part) {
            Some(c) => {
                cmd = Some(c);
                current = &c.commands;
            }
            None => bail!("command '{}' not found", path),
        }
    }
    cmd.ok_or_else(|| anyhow!("empty command path"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── build_clap_command tests ────────────────────────────────────

    #[test]
    fn test_build_clap_single_default() {
        let yaml = r#"
name: echo
version: "0.2.0"
description: "Echo text"
commands:
  default:
    description: Echo text
    args:
      text:
        type: string
        required: true
"#;
        let def = crate::package_definition::parse(yaml).unwrap();
        let cmd = build_clap_command(&def);

        // Should accept positional arg, no "default" subcommand
        let matches = cmd.try_get_matches_from(["echo", "hello"]).unwrap();
        assert_eq!(matches.get_one::<String>("text").unwrap(), "hello");
        assert!(matches.subcommand().is_none());
    }

    #[test]
    fn test_build_clap_default_plus_named() {
        let yaml = r#"
name: my-pkg
version: "1.0.0"
commands:
  default:
    args:
      text:
        type: string
  transcribe:
    description: Transcribe audio
    args:
      file:
        type: path
        required: true
"#;
        let def = crate::package_definition::parse(yaml).unwrap();

        // No subcommand: uses default's args
        let cmd = build_clap_command(&def);
        let matches = cmd.try_get_matches_from(["my-pkg", "hello"]).unwrap();
        assert!(matches.subcommand().is_none());
        assert_eq!(matches.get_one::<String>("text").unwrap(), "hello");

        // Named subcommand works
        let cmd = build_clap_command(&def);
        let matches = cmd
            .try_get_matches_from(["my-pkg", "transcribe", "file.mp3"])
            .unwrap();
        let (name, sub) = matches.subcommand().unwrap();
        assert_eq!(name, "transcribe");
        assert_eq!(sub.get_one::<String>("file").unwrap(), "file.mp3");
    }

    #[test]
    fn test_build_clap_named_only() {
        let yaml = r#"
name: my-pkg
version: "1.0.0"
commands:
  transcribe:
    description: Transcribe audio
  translate:
    description: Translate text
"#;
        let def = crate::package_definition::parse(yaml).unwrap();
        let cmd = build_clap_command(&def);

        // No subcommand should error
        let result = cmd.try_get_matches_from(["my-pkg"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_clap_nested_commands() {
        let yaml = r#"
name: my-pkg
version: "1.0.0"
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
"#;
        let def = crate::package_definition::parse(yaml).unwrap();
        let cmd = build_clap_command(&def);

        let matches = cmd
            .try_get_matches_from(["my-pkg", "transcribe", "batch", "*.mp3"])
            .unwrap();
        let (name, sub) = matches.subcommand().unwrap();
        assert_eq!(name, "transcribe");
        let (nested_name, nested_sub) = sub.subcommand().unwrap();
        assert_eq!(nested_name, "batch");
        assert_eq!(nested_sub.get_one::<String>("files").unwrap(), "*.mp3");
    }

    #[test]
    fn test_build_clap_arg_types() {
        let yaml = r#"
name: test
version: "1.0.0"
commands:
  default:
    args:
      input:
        type: string
        required: true
      count:
        type: number
        flag: count
      verbose:
        type: bool
        flag: verbose
      file:
        type: path
        flag: file
      format:
        type: choice
        flag: format
        values: [json, csv, text]
"#;
        let def = crate::package_definition::parse(yaml).unwrap();

        // Number validation rejects non-numeric
        let cmd = build_clap_command(&def);
        let result = cmd.try_get_matches_from(["test", "hello", "--count", "abc"]);
        assert!(result.is_err());

        // Choice validation rejects invalid value
        let cmd = build_clap_command(&def);
        let result = cmd.try_get_matches_from(["test", "hello", "--format", "invalid"]);
        assert!(result.is_err());

        // Valid args parse correctly
        let cmd = build_clap_command(&def);
        let matches = cmd
            .try_get_matches_from([
                "test",
                "hello",
                "--count",
                "42",
                "--verbose",
                "--file",
                "/path",
                "--format",
                "json",
            ])
            .unwrap();
        assert_eq!(matches.get_one::<String>("input").unwrap(), "hello");
        assert_eq!(matches.get_one::<String>("count").unwrap(), "42");
        assert!(matches.get_flag("verbose"));
        assert_eq!(matches.get_one::<String>("file").unwrap(), "/path");
        assert_eq!(matches.get_one::<String>("format").unwrap(), "json");
    }

    #[test]
    fn test_build_clap_flag_vs_positional() {
        let yaml = r#"
name: test
version: "1.0.0"
commands:
  default:
    args:
      text:
        type: string
        required: true
      model:
        type: choice
        flag: model
        values: [base, large]
"#;
        let def = crate::package_definition::parse(yaml).unwrap();

        // Positional + flag
        let cmd = build_clap_command(&def);
        let matches = cmd
            .try_get_matches_from(["test", "hello", "--model", "large"])
            .unwrap();
        assert_eq!(matches.get_one::<String>("text").unwrap(), "hello");
        assert_eq!(matches.get_one::<String>("model").unwrap(), "large");

        // Flag before positional
        let cmd = build_clap_command(&def);
        let matches = cmd
            .try_get_matches_from(["test", "--model", "base", "hello"])
            .unwrap();
        assert_eq!(matches.get_one::<String>("text").unwrap(), "hello");
        assert_eq!(matches.get_one::<String>("model").unwrap(), "base");
    }

    // ─── clap_parse tests ────────────────────────────────────────────

    #[test]
    fn test_clap_parse_default_command() {
        let yaml = r#"
name: echo
version: "0.2.0"
commands:
  default:
    args:
      text:
        type: string
        required: true
"#;
        let def = crate::package_definition::parse(yaml).unwrap();
        let cmd = build_clap_command(&def);
        let (path, flags) = clap_parse(cmd, "echo", &["hello".to_string()], &def).unwrap();
        assert_eq!(path, "default");
        assert_eq!(flags["text"], "hello");
    }

    #[test]
    fn test_clap_parse_named_command() {
        let yaml = r#"
name: my-pkg
version: "1.0.0"
commands:
  transcribe:
    description: Transcribe audio
    args:
      file:
        type: path
        required: true
  translate:
    description: Translate text
"#;
        let def = crate::package_definition::parse(yaml).unwrap();
        let cmd = build_clap_command(&def);
        let (path, flags) = clap_parse(
            cmd,
            "my-pkg",
            &["transcribe".to_string(), "file.mp3".to_string()],
            &def,
        )
        .unwrap();
        assert_eq!(path, "transcribe");
        assert_eq!(flags["file"], "file.mp3");
    }

    #[test]
    fn test_clap_parse_nested_command() {
        let yaml = r#"
name: my-pkg
version: "1.0.0"
commands:
  transcribe:
    description: Transcribe
    commands:
      batch:
        description: Batch
        args:
          files:
            type: string
            required: true
"#;
        let def = crate::package_definition::parse(yaml).unwrap();
        let cmd = build_clap_command(&def);
        let (path, flags) = clap_parse(
            cmd,
            "my-pkg",
            &[
                "transcribe".to_string(),
                "batch".to_string(),
                "*.mp3".to_string(),
            ],
            &def,
        )
        .unwrap();
        assert_eq!(path, "transcribe.batch");
        assert_eq!(flags["files"], "*.mp3");
    }

    #[test]
    fn test_clap_parse_bool_flag() {
        let yaml = r#"
name: test
version: "1.0.0"
commands:
  default:
    args:
      verbose:
        type: bool
        flag: verbose
"#;
        let def = crate::package_definition::parse(yaml).unwrap();

        // With flag
        let cmd = build_clap_command(&def);
        let (_, flags) = clap_parse(cmd, "test", &["--verbose".to_string()], &def).unwrap();
        assert_eq!(flags["verbose"], "true");

        // Without flag
        let cmd = build_clap_command(&def);
        let (_, flags) = clap_parse(cmd, "test", &[], &def).unwrap();
        assert!(!flags.contains_key("verbose"));
    }

    #[test]
    fn test_clap_parse_unknown_flag_error() {
        let yaml = r#"
name: echo
version: "0.2.0"
commands:
  default:
    args:
      text:
        type: string
"#;
        let def = crate::package_definition::parse(yaml).unwrap();
        let cmd = build_clap_command(&def);
        let result = clap_parse(
            cmd,
            "echo",
            &["--unknown".to_string(), "hello".to_string()],
            &def,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_bool_auto_flags() {
        let yaml = r#"
name: test
version: "1.0.0"
commands:
  default:
    args:
      changes:
        type: bool
      drafts:
        type: bool
"#;
        let def = crate::package_definition::parse(yaml).unwrap();

        // Bools without explicit flag: become --flags automatically
        let cmd = build_clap_command(&def);
        let (_, flags) = clap_parse(cmd, "test", &["--changes".to_string()], &def).unwrap();
        assert_eq!(flags["changes"], "true");
        assert!(!flags.contains_key("drafts"));

        // Both flags
        let cmd = build_clap_command(&def);
        let (_, flags) = clap_parse(
            cmd,
            "test",
            &["--changes".to_string(), "--drafts".to_string()],
            &def,
        )
        .unwrap();
        assert_eq!(flags["changes"], "true");
        assert_eq!(flags["drafts"], "true");

        // No flags
        let cmd = build_clap_command(&def);
        let (_, flags) = clap_parse(cmd, "test", &[], &def).unwrap();
        assert!(!flags.contains_key("changes"));
        assert!(!flags.contains_key("drafts"));
    }

    // ─── find_command tests ──────────────────────────────────────────

    #[test]
    fn test_find_command_default() {
        let mut commands = BTreeMap::new();
        commands.insert("default".to_string(), CommandDefinition::default());
        let cmd = find_command(&commands, "default").unwrap();
        assert!(cmd.args.is_empty());
    }

    #[test]
    fn test_find_command_nested() {
        let mut inner = BTreeMap::new();
        inner.insert("batch".to_string(), CommandDefinition::default());
        let mut commands = BTreeMap::new();
        commands.insert(
            "transcribe".to_string(),
            CommandDefinition {
                commands: inner,
                ..Default::default()
            },
        );
        let cmd = find_command(&commands, "transcribe.batch").unwrap();
        assert!(cmd.args.is_empty());
    }

    #[test]
    fn test_find_command_not_found() {
        let commands = BTreeMap::new();
        let result = find_command(&commands, "nonexistent");
        assert!(result.is_err());
    }
}
// This file is path-included by both the `zacor` and `zr` crates. Some
// dispatch helpers are only exercised by `zr`, while `zacor` only uses the
// clap command builder for completions.
