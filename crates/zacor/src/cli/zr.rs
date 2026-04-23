use crate::{daemon_client, dispatch, paths, receipt};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Text,
    Plain,
    Json,
}

impl From<OutputFormat> for dispatch::OutputMode {
    fn from(value: OutputFormat) -> Self {
        match value {
            OutputFormat::Text => dispatch::OutputMode::Text,
            OutputFormat::Plain => dispatch::OutputMode::Plain,
            OutputFormat::Json => dispatch::OutputMode::Json,
        }
    }
}

#[derive(Parser)]
#[command(
    name = "zr",
    version,
    about = "Module dispatcher",
    disable_help_subcommand = true,
    allow_external_subcommands = true
)]
struct ZrCli {
    /// Output JSONL records instead of human-readable rendering
    #[arg(long, conflicts_with_all = ["text", "format"])]
    json: bool,

    /// Force rich human-readable output even when piped
    #[arg(long, conflicts_with_all = ["json", "plain", "format"])]
    text: bool,

    /// Force minimal human-readable output even when piped
    #[arg(long, conflicts_with_all = ["json", "text", "format"])]
    plain: bool,

    /// Select output format explicitly: text, plain, or json
    #[arg(long, value_enum, conflicts_with_all = ["json", "text", "plain"])]
    format: Option<OutputFormat>,

    #[command(subcommand)]
    command: Option<ZrCommand>,
}

impl ZrCli {
    fn output_mode(&self) -> dispatch::OutputMode {
        if let Some(format) = self.format {
            format.into()
        } else if self.json {
            dispatch::OutputMode::Json
        } else if self.plain {
            dispatch::OutputMode::Plain
        } else if self.text {
            dispatch::OutputMode::Text
        } else {
            dispatch::OutputMode::Auto
        }
    }
}

#[derive(Subcommand)]
enum ZrCommand {
    /// Manage the zr daemon
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },

    #[command(external_subcommand)]
    Module(Vec<String>),
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start the daemon (foreground)
    Start,
    /// Stop a running daemon
    Stop,
    /// Show daemon and service status
    Status,
}

/// Build the dynamic "Available packages" section for `zr --help`.
fn build_package_list_help(home: &Path) -> String {
    let mut out = String::new();

    if let Ok(packages) = receipt::list_all(home) {
        let active: Vec<_> = packages.iter().filter(|(_, r)| r.active).collect();
        if active.is_empty() {
            out.push_str("No packages installed. Use `zacor install` to add packages.");
        } else {
            out.push_str("Available packages:");
            for (name, r) in active {
                out.push_str(&format!("\n  {}  v{}", name, r.current));
            }
        }
    } else {
        out.push_str("No packages installed. Use `zacor install` to add packages.");
    }

    out
}

pub fn run() -> i32 {
    let home = match paths::zr_home() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("error: {:#}", e);
            return 1;
        }
    };

    if let Err(e) = paths::ensure_dirs(&home) {
        eprintln!("error: {:#}", e);
        return 1;
    }

    let package_list = build_package_list_help(&home);
    let cli = match ZrCli::command().after_help(package_list).try_get_matches() {
        Ok(matches) => match ZrCli::from_arg_matches(&matches) {
            Ok(cli) => cli,
            Err(e) => {
                eprintln!("{}", e);
                return 1;
            }
        },
        Err(e) => {
            if e.use_stderr() {
                eprintln!("{}", e);
                return 1;
            } else {
                print!("{}", e);
                return 0;
            }
        }
    };

    let output_mode = cli.output_mode();

    match cli.command {
        Some(ZrCommand::Daemon { action }) => run_daemon(&home, action),
        Some(ZrCommand::Module(args)) => {
            let module_name = &args[0];
            let module_args: Vec<String> = args[1..].to_vec();
            match dispatch::run(&home, module_name, &module_args, output_mode) {
                Ok(code) => code,
                Err(e) => {
                    eprintln!("error: {:#}", e);
                    1
                }
            }
        }
        None => {
            // No command given — show help with package list
            let package_list = build_package_list_help(&home);
            let _ = ZrCli::command().after_help(package_list).print_help();
            println!();
            0
        }
    }
}

fn run_daemon(home: &Path, action: DaemonAction) -> i32 {
    match action {
        DaemonAction::Start => {
            match std::process::Command::new(crate::resolve_peer_binary("zr-daemon"))
                .env("ZR_HOME", home)
                .status()
            {
                Ok(status) => status.code().unwrap_or(1),
                Err(e) => {
                    eprintln!("error: failed to run `zr-daemon`: {}", e);
                    1
                }
            }
        }
        DaemonAction::Stop => match daemon_client::connect() {
            Some(stream) => match daemon_client::shutdown(&stream) {
                Ok(_) => {
                    println!("daemon stopped");
                    0
                }
                Err(e) => {
                    if e.to_string().contains("connection") {
                        println!("daemon stopped");
                        0
                    } else {
                        eprintln!("error: {:#}", e);
                        1
                    }
                }
            },
            None => {
                println!("daemon is not running");
                0
            }
        },
        DaemonAction::Status => match daemon_client::connect() {
            Some(stream) => match daemon_client::status(&stream) {
                Ok(resp) => {
                    println!("daemon: running");
                    if let Some(services) = resp.services {
                        if services.is_empty() {
                            println!("services: none");
                        } else {
                            println!("services:");
                            for svc in services {
                                println!("  {} - port {} ({})", svc.name, svc.port, svc.status);
                            }
                        }
                    }
                    0
                }
                Err(e) => {
                    eprintln!("error: {:#}", e);
                    1
                }
            },
            None => {
                println!("daemon: not running");
                0
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_mode_defaults_to_auto() {
        let cli = ZrCli::try_parse_from(["zr", "pkg"]).unwrap();
        assert_eq!(cli.output_mode(), dispatch::OutputMode::Auto);
    }

    #[test]
    fn test_format_plain_sets_plain_mode() {
        let cli = ZrCli::try_parse_from(["zr", "--format", "plain", "pkg"]).unwrap();
        assert_eq!(cli.output_mode(), dispatch::OutputMode::Plain);
    }

    #[test]
    fn test_json_alias_still_sets_json_mode() {
        let cli = ZrCli::try_parse_from(["zr", "--json", "pkg"]).unwrap();
        assert_eq!(cli.output_mode(), dispatch::OutputMode::Json);
    }

    #[test]
    fn test_plain_alias_still_sets_plain_mode() {
        let cli = ZrCli::try_parse_from(["zr", "--plain", "pkg"]).unwrap();
        assert_eq!(cli.output_mode(), dispatch::OutputMode::Plain);
    }

    #[test]
    fn test_output_flags_conflict() {
        assert!(ZrCli::try_parse_from(["zr", "--text", "--json", "pkg"]).is_err());
        assert!(ZrCli::try_parse_from(["zr", "--plain", "--json", "pkg"]).is_err());
        assert!(ZrCli::try_parse_from(["zr", "--plain", "--text", "pkg"]).is_err());
        assert!(ZrCli::try_parse_from(["zr", "--format", "plain", "--text", "pkg"]).is_err());
        assert!(ZrCli::try_parse_from(["zr", "--format", "plain", "--plain", "pkg"]).is_err());
        assert!(ZrCli::try_parse_from(["zr", "--format", "json", "--json", "pkg"]).is_err());
    }

    #[test]
    fn test_invalid_format_value_fails() {
        assert!(ZrCli::try_parse_from(["zr", "--format", "invalid", "pkg"]).is_err());
    }
}
