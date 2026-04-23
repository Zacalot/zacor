use crate::{dispatch_clap_builder, paths, receipt};
use clap::{Parser, Subcommand};
use std::process::Command;

#[derive(Parser)]
#[command(name = "zacor", version, about = "Package manager for zr")]
struct ZacorCli {
    #[command(subcommand)]
    command: ZacorCommand,
}

#[derive(Subcommand)]
enum ZacorCommand {
    /// Install a package from a source
    Install {
        /// Source: package name, name@version, github.com/owner/repo[@version], archive path, .yaml definition, or git URL
        source: String,
        /// Override the package name
        #[arg(long)]
        name: Option<String>,
        /// Replace existing package
        #[arg(long)]
        force: bool,
        /// Install without activating
        #[arg(long)]
        inactive: bool,
        /// Build from source instead of downloading release assets
        #[arg(long)]
        from_source: bool,
    },

    /// Remove an installed package (or specific version with name@version)
    Remove {
        /// Package name (or name@version)
        name: String,
        /// Force removal even if dependents exist
        #[arg(long)]
        force: bool,
    },

    /// List installed packages
    List,

    /// Enable an installed package
    Enable {
        /// Package name
        name: String,
    },

    /// Disable an active package
    Disable {
        /// Package name
        name: String,
    },

    /// Update a package to the latest version
    Update {
        /// Package name
        name: String,
    },

    /// Switch the current version of a package
    Use {
        /// Package name
        name: String,
        /// Version to switch to
        version: String,
    },

    /// Get, set, or unset package configuration
    Config {
        /// Key in package.key format (e.g. my-pkg.model)
        key: Option<String>,
        /// Value to set
        value: Option<String>,
        /// Remove the config key
        #[arg(long)]
        unset: bool,
        /// Operate on global config.toml
        #[arg(long)]
        global: bool,
        /// List all config for a package
        #[arg(long)]
        list: bool,
        /// Open config.toml in $EDITOR
        #[arg(long)]
        edit: bool,
    },

    /// Start the HTTP server for remote package execution
    Serve {
        /// Port to listen on
        #[arg(long, default_value = "8787")]
        port: u16,
        /// Address to bind to
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
    },

    /// Manage the service daemon
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },

    /// Initialize a zr project and sync supported features
    Init {
        /// Target features (e.g. claude-code gemini). Detects existing features if omitted.
        features: Vec<String>,
    },

    /// Manage package registries
    Registry {
        #[command(subcommand)]
        action: RegistryAction,
    },

    /// Generate shell completion scripts
    Completions {
        /// Shell to generate completions for (bash, zsh, fish, powershell)
        shell: String,
    },
}

#[derive(Subcommand)]
enum RegistryAction {
    /// Add a registry
    Add {
        /// Registry URL
        url: String,
        /// Custom name for the registry
        #[arg(long)]
        name: Option<String>,
    },
    /// Remove a registry
    Remove {
        /// Registry name
        name: String,
    },
    /// List configured registries
    List,
    /// Sync registry index
    Sync {
        /// Sync a specific registry by name
        #[arg(long)]
        name: Option<String>,
    },
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

pub fn run() -> i32 {
    let cli = ZacorCli::parse();
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

    let result = match cli.command {
        ZacorCommand::Install {
            source: src,
            name,
            force,
            inactive,
            from_source,
        } => super::install::run(&home, &src, name.as_deref(), force, inactive, from_source),
        ZacorCommand::Remove { name, force } => super::remove::run(&home, &name, force),
        ZacorCommand::List => super::list::run(&home),
        ZacorCommand::Enable { name } => super::enable::run(&home, &name),
        ZacorCommand::Disable { name } => super::disable::run(&home, &name),
        ZacorCommand::Update { name } => super::update::run(&name),
        ZacorCommand::Use { name, version } => super::use_cmd::run(&home, &name, &version),
        ZacorCommand::Config {
            key,
            value,
            unset,
            global,
            list,
            edit,
        } => super::config_cmd::run(
            &home,
            key.as_deref(),
            value.as_deref(),
            unset,
            global,
            list,
            edit,
        ),
        ZacorCommand::Init { features } => super::init::run(&home, &features),
        ZacorCommand::Registry { action } => match action {
            RegistryAction::Add { url, name } => {
                super::registry_cmd::add(&home, &url, name.as_deref())
            }
            RegistryAction::Remove { name } => super::registry_cmd::remove(&home, &name),
            RegistryAction::List => super::registry_cmd::list(&home),
            RegistryAction::Sync { name } => super::registry_cmd::sync(&home, name.as_deref()),
        },
        ZacorCommand::Serve { port, bind } => crate::serve::run(&home, &bind, port),
        ZacorCommand::Daemon { action } => {
            return run_daemon(&home, action);
        }
        ZacorCommand::Completions { shell } => {
            return run_completions(&home, &shell);
        }
    };

    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {:#}", e);
            1
        }
    }
}

fn run_daemon(home: &std::path::Path, action: DaemonAction) -> i32 {
    match action {
        DaemonAction::Start => match Command::new(crate::resolve_zr_daemon_binary())
            .env("ZR_HOME", home)
            .status()
        {
            Ok(status) => status.code().unwrap_or(1),
            Err(e) => {
                eprintln!("error: failed to run `zr-daemon`: {}", e);
                1
            }
        },
        DaemonAction::Stop => match Command::new(crate::resolve_zr_binary())
            .env("ZR_HOME", home)
            .args(["daemon", "stop"])
            .status()
        {
            Ok(status) => status.code().unwrap_or(1),
            Err(e) => {
                eprintln!("error: failed to run `zr daemon stop`: {}", e);
                1
            }
        },
        DaemonAction::Status => match Command::new(crate::resolve_zr_binary())
            .env("ZR_HOME", home)
            .args(["daemon", "status"])
            .status()
        {
            Ok(status) => status.code().unwrap_or(1),
            Err(e) => {
                eprintln!("error: failed to run `zr daemon status`: {}", e);
                1
            }
        },
    }
}

fn run_completions(home: &std::path::Path, shell_name: &str) -> i32 {
    let shell = match shell_name {
        "bash" => clap_complete::Shell::Bash,
        "zsh" => clap_complete::Shell::Zsh,
        "fish" => clap_complete::Shell::Fish,
        "powershell" => clap_complete::Shell::PowerShell,
        _ => {
            eprintln!(
                "error: unsupported shell '{}'\nsupported shells: bash, zsh, fish, powershell",
                shell_name
            );
            return 1;
        }
    };

    let mut zr_cmd = clap::Command::new("zr");

    if let Ok(packages) = receipt::list_all(home) {
        for (name, r) in &packages {
            if !r.active {
                continue;
            }
            if let Ok(def) = crate::wasm_manifest::load_from_store(home, name, &r.current) {
                let pkg_cmd = dispatch_clap_builder::build_clap_command(&def);
                zr_cmd = zr_cmd.subcommand(pkg_cmd);
            }
        }
    }

    clap_complete::generate(shell, &mut zr_cmd, "zr", &mut std::io::stdout());
    0
}
