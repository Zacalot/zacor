mod clap_builder;
mod session;

use crate::config;
use crate::error::*;
use crate::package_definition::{CommandDefinition, OutputDeclaration, PackageDefinition};
use crate::paths;
use crate::receipt::{self, Receipt};
use crate::render::RenderMode;
use crate::wasm_runtime;
use std::collections::BTreeMap;
use std::io::{BufReader, IsTerminal};
use std::net::TcpStream;
use std::path::Path;
use std::process::{self, Command, Stdio};
use zacor_host::capability::CapabilityRegistry;
use zacor_host::router::{InvocationOutcome, PackageRouter};
use zacor_package::protocol::{self as proto, Message};

pub use clap_builder::build_clap_command;
use clap_builder::{clap_parse, find_command};
use session::{
    CallbackOutputHandler, resolve_render_mode, run_protocol_session,
    run_protocol_session_with_handler,
};

/// A resolved package ready for dispatch.
pub struct ResolvedPackage {
    pub receipt: Receipt,
    pub definition: PackageDefinition,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Auto,
    Text,
    Plain,
    Json,
}

// ─── Resolve Phase ───────────────────────────────────────────────────

fn resolve(home: &Path, name: &str) -> Result<ResolvedPackage> {
    let receipt = match receipt::read(home, name)? {
        Some(receipt) => receipt,
        None => return Err(crate::error::DispatchError::PackageNotFound(name.into()).into()),
    };

    if !receipt.active {
        return Err(crate::error::DispatchError::Disabled(name.into()).into());
    }

    let version = receipt.current.clone();
    let store_dir = paths::store_path(home, name, &version);

    if !store_dir.is_dir() {
        bail!(
            "store directory for '{}' v{} not found\nhint: reinstall with `zacor install <source>`",
            name,
            version
        );
    }

    let definition = crate::wasm_manifest::load_from_store(home, name, &version).with_context(|| {
        format!(
            "corrupt manifest for '{}' v{}\nhint: reinstall with `zacor install <source>`",
            name, version
        )
    })?;

    Ok(ResolvedPackage {
        receipt,
        definition,
        version,
    })
}

// ─── Execute Phase ───────────────────────────────────────────────────

/// Resolve the effective execution mode: receipt mode > execution.default > "command".
fn resolve_mode(resolved: &ResolvedPackage) -> receipt::Mode {
    // 1. Receipt mode takes priority
    if let Some(mode) = resolved.receipt.mode {
        return mode;
    }
    // 2. execution.default from package.yaml
    if let Some(ref exec) = resolved.definition.execution {
        if let Some(ref default) = exec.default {
            if let Ok(mode) = default.parse::<receipt::Mode>() {
                return mode;
            }
        }
    }
    // 3. Fallback to command
    receipt::Mode::Command
}

fn verify_dep_declared(caller: Option<&PackageDefinition>, package: &str) -> Result<()> {
    let Some(caller) = caller else {
        return Ok(());
    };
    if caller.name == package {
        return Ok(());
    }
    if caller.depends.packages.iter().any(|dep| dep.name == package) {
        return Ok(());
    }

    bail!(
        "package '{}' is not declared in depends.packages for '{}'",
        package,
        caller.name
    )
}

#[allow(clippy::too_many_arguments)]
fn execute(
    home: &Path,
    resolved: &ResolvedPackage,
    env_vars: &BTreeMap<String, String>,
    placeholders: &BTreeMap<String, String>,
    command_path: &str,
    command: &CommandDefinition,
    parsed_flags: &BTreeMap<String, String>,
    output_mode: OutputMode,
    capabilities: &CapabilityRegistry,
    package_router: Option<&dyn PackageRouter>,
    depth: usize,
    max_depth: usize,
) -> Result<i32> {
    // Wasm packages always speak the module protocol. Route before the
    // native protocol branch so the `wasm` field short-circuits the
    // `binary`/`run` resolve path, which doesn't apply.
    if resolved.definition.wasm.is_some() {
        return execute_wasm(
            home,
            resolved,
            command_path,
            command,
            parsed_flags,
            output_mode,
            env_vars,
            capabilities,
            package_router,
            depth,
            max_depth,
        );
    }

    // Protocol packages use the new module protocol
    if resolved.definition.protocol {
        let mode = resolve_mode(resolved);
        if mode == receipt::Mode::Service && resolved.definition.service.is_some() {
            return execute_service(
                home,
                resolved,
                command_path,
                command,
                parsed_flags,
                output_mode,
                capabilities,
                package_router,
                depth,
                max_depth,
            );
        }
        return execute_protocol(
            home,
            resolved,
            command_path,
            command,
            parsed_flags,
            output_mode,
            env_vars,
            capabilities,
            package_router,
            depth,
            max_depth,
        );
    }

    // Legacy path: env vars + raw stdout
    execute_command(home, resolved, env_vars, placeholders, command, output_mode)
}

// ─── Service Dispatch ────────────────────────────────────────────────

fn execute_service(
    home: &Path,
    resolved: &ResolvedPackage,
    command_path: &str,
    command: &CommandDefinition,
    parsed_flags: &BTreeMap<String, String>,
    output_mode: OutputMode,
    capabilities: &CapabilityRegistry,
    package_router: Option<&dyn PackageRouter>,
    depth: usize,
    max_depth: usize,
) -> Result<i32> {
    let service = resolved.definition.service.as_ref().unwrap();
    let port = service.port.ok_or_else(|| {
        anyhow!(
            "service package '{}' must declare a port in service.port",
            resolved.definition.name
        )
    })?;

    // Ensure the service is running (starts daemon + service if needed)
    ensure_service_running(home, &resolved.definition.name, port)?;

    // Connect to the running service via TCP
    let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).with_context(|| {
        format!(
            "failed to connect to service '{}' on port {}",
            resolved.definition.name, port
        )
    })?;
    let reader = BufReader::new(stream.try_clone().context("failed to clone TCP stream")?);

    // Build INVOKE message
    let has_input = command.input.is_some();
    let invoke_msg = Message::Invoke(proto::Invoke::from_str_args(
        command_path,
        parsed_flags,
        has_input,
    ));

    // Run protocol session over TCP
    run_protocol_session(
        reader,
        stream,
        &invoke_msg,
        &resolved.definition,
        command,
        output_mode,
        capabilities,
        package_router,
        depth,
        max_depth,
    )
}

const DEFAULT_MAX_CALL_DEPTH: usize = 8;

fn validate_command_input_mode_for_stdin(
    command: &CommandDefinition,
    stdin_is_terminal: bool,
) -> Result<()> {
    if command.input.is_some() && command.inline_input_fallback.is_none() && stdin_is_terminal {
        bail!(
            "this command requires piped stdin; provide input via a pipe or file-backed stdin"
        );
    }

    Ok(())
}

fn validate_command_input_mode(command: &CommandDefinition) -> Result<()> {
    validate_command_input_mode_for_stdin(command, std::io::stdin().is_terminal())
}

struct LocalPackageRouter<'a> {
    home: &'a Path,
    capabilities: &'a CapabilityRegistry,
}

impl PackageRouter for LocalPackageRouter<'_> {
    fn invoke(
        &self,
        caller: Option<&PackageDefinition>,
        package: &str,
        command: &str,
        args: &BTreeMap<String, String>,
        depth: usize,
        max_depth: usize,
        on_output: &mut dyn FnMut(serde_json::Value) -> std::result::Result<(), String>,
    ) -> InvocationOutcome {
        if let Err(error) = verify_dep_declared(caller, package) {
            return InvocationOutcome::failure(format!("{:#}", error));
        }

        match invoke_package_local(
            self.home,
            package,
            command,
            args,
            self.capabilities,
            depth,
            max_depth,
            self,
            on_output,
        ) {
            Ok(exit_code) => InvocationOutcome::success(exit_code),
            Err(error) => InvocationOutcome::failure(format!("{:#}", error)),
        }
    }
}

/// Ensure a service is running by contacting the daemon.
/// Starts the daemon lazily if it is not running.
fn ensure_service_running(home: &Path, name: &str, port: u16) -> Result<()> {
    // Try connecting to the service directly first (fast path)
    if TcpStream::connect(format!("127.0.0.1:{}", port)).is_ok() {
        return Ok(());
    }

    // Contact daemon, start it lazily if needed
    let client = crate::daemon_client::connect_or_start_daemon(home)?;

    // Ask daemon to start the service
    let response = crate::daemon_client::start_service(&client, name)?;
    if !response.ok {
        bail!(
            "failed to start service '{}': {}",
            name,
            response.error.unwrap_or_else(|| "unknown error".into())
        );
    }

    Ok(())
}

// ─── Protocol Dispatch ───────────────────────────────────────────────

/// Resolve the launch command for a protocol package.
/// Returns `(program, args)` — either from `binary` (single executable) or `run` (tokenized command string).
fn resolve_launch_command(
    home: &Path,
    def: &PackageDefinition,
    version: &str,
) -> Result<(String, Vec<String>)> {
    let store_dir = paths::store_path(home, &def.name, version);

    if let Some(ref binary_name) = def.binary {
        let bin_path = paths::store_binary_path(home, &def.name, version, binary_name);
        if !bin_path.exists() {
            bail!(
                "binary '{}' not found for '{}' v{}\nhint: reinstall with `zacor install <source>`",
                binary_name,
                def.name,
                version
            );
        }
        Ok((bin_path.to_string_lossy().into_owned(), vec![]))
    } else if let Some(ref run_cmd) = def.run {
        let tokens = shlex::split(run_cmd).ok_or_else(|| {
            anyhow!(
                "failed to parse run command for '{}': {}",
                def.name,
                run_cmd
            )
        })?;
        if tokens.is_empty() {
            bail!("run command for '{}' is empty", def.name);
        }
        let resolved: Vec<String> = tokens
            .into_iter()
            .map(|token| {
                let candidate = store_dir.join(&token);
                if candidate.exists() {
                    // Use forward slashes so interpreted shells (bash, sh) work on Windows
                    candidate.to_string_lossy().replace('\\', "/")
                } else {
                    token
                }
            })
            .collect();
        let (program, args) = resolved.split_first().unwrap();
        Ok((program.clone(), args.to_vec()))
    } else {
        bail!(
            "protocol package '{}' must have either 'run' or 'binary'",
            def.name
        );
    }
}

fn execute_protocol(
    home: &Path,
    resolved: &ResolvedPackage,
    command_path: &str,
    command: &CommandDefinition,
    parsed_flags: &BTreeMap<String, String>,
    output_mode: OutputMode,
    env_vars: &BTreeMap<String, String>,
    capabilities: &CapabilityRegistry,
    package_router: Option<&dyn PackageRouter>,
    depth: usize,
    max_depth: usize,
) -> Result<i32> {
    let (program, args) = resolve_launch_command(home, &resolved.definition, &resolved.version)?;

    // Set up Job Object on Windows so child dies when zr exits
    #[cfg(windows)]
    let _job = crate::job_object::JobObject::setup().ok();

    // Spawn module with piped stdin/stdout, inherited stderr
    let mut child = Command::new(&program)
        .args(&args)
        .envs(env_vars)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn package '{}'", resolved.definition.name))?;

    #[cfg(windows)]
    if let Some(ref job) = _job {
        let _ = job.assign(&child);
    }

    let child_stdin = child.stdin.take().unwrap();
    let child_stdout = child.stdout.take().unwrap();

    // Build INVOKE message
    let has_input = command.input.is_some();
    let invoke_msg = Message::Invoke(proto::Invoke::from_str_args(
        command_path,
        parsed_flags,
        has_input,
    ));

    // Run protocol session over child stdio
    let reader = BufReader::new(child_stdout);
    let result = run_protocol_session(
        reader,
        child_stdin,
        &invoke_msg,
        &resolved.definition,
        command,
        output_mode,
        capabilities,
        package_router,
        depth,
        max_depth,
    );

    // If session ended without a DONE (e.g. crash), use process exit code
    let _ = child.wait();
    result
}

// ─── Wasm Dispatch ───────────────────────────────────────────────────

fn execute_wasm(
    home: &Path,
    resolved: &ResolvedPackage,
    command_path: &str,
    command: &CommandDefinition,
    parsed_flags: &BTreeMap<String, String>,
    output_mode: OutputMode,
    env_vars: &BTreeMap<String, String>,
    capabilities: &CapabilityRegistry,
    package_router: Option<&dyn PackageRouter>,
    depth: usize,
    max_depth: usize,
) -> Result<i32> {
    let wasm_name = resolved.definition.wasm.as_ref().ok_or_else(|| {
        anyhow!(
            "internal error: execute_wasm called for non-wasm package '{}'",
            resolved.definition.name
        )
    })?;

    let wasm_path = paths::store_wasm_path(home, &resolved.definition.name, &resolved.version, wasm_name);
    if !wasm_path.exists() {
        return Err(crate::error::DispatchError::ArtifactMissing(wasm_path).into());
    }

    let debug_timing = std::env::var("ZR_DEBUG_TIMING").is_ok();
    let t0 = std::time::Instant::now();

    // Build INVOKE message (same regardless of dispatch path).
    let has_input = command.input.is_some();
    let invoke_msg = Message::Invoke(proto::Invoke::from_str_args(
        command_path,
        parsed_flags,
        has_input,
    ));

    // Try daemon-hosted dispatch first — skips wasmtime init, uses the
    // daemon's hot module cache. Falls back to in-process on any daemon
    // error so single-user single-invocation cases still work without
    // requiring an explicit `zacor daemon start`.
    match crate::daemon_client::try_open_dispatch_stream(
        home,
        &resolved.definition.name,
        &resolved.version,
        env_vars,
    ) {
        Ok(Some(stream)) => {
            if debug_timing {
                eprintln!("  [wasm] daemon open:    {:?}", t0.elapsed());
            }
            let tcp_reader = std::io::BufReader::new(
                stream
                    .try_clone()
                    .context("cloning daemon stream for session read")?,
            );
            let tcp_writer = stream;
            let t_session = std::time::Instant::now();
            let result = run_protocol_session(
                tcp_reader,
                tcp_writer,
                &invoke_msg,
                &resolved.definition,
                command,
                output_mode,
                capabilities,
                package_router,
                depth,
                max_depth,
            );
            if debug_timing {
                eprintln!("  [wasm] daemon session: {:?}", t_session.elapsed());
                eprintln!("  [wasm] TOTAL (daemon): {:?}", t0.elapsed());
            }
            return result;
        }
        Ok(None) => {
            if debug_timing {
                eprintln!("  [wasm] daemon: not running, using in-process");
            }
        }
        Err(e) => {
            eprintln!(
                "warning: daemon dispatch failed — falling back to in-process: {:#}",
                e
            );
        }
    }

    // In-process fallback.
    let host = wasm_runtime::WasmHost::shared()?;
    let module = host.load_module(&wasm_path)?;

    let env: Vec<(String, String)> = env_vars
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let wasm_runtime::WasmSession {
        writer,
        reader,
        controller,
    } = host.invoke(module, env)?;

    let result = run_protocol_session(
        reader,
        writer,
        &invoke_msg,
        &resolved.definition,
        command,
        output_mode,
        capabilities,
        package_router,
        depth,
        max_depth,
    );
    let _ = controller.finish();
    if debug_timing {
        eprintln!("  [wasm] TOTAL (inproc): {:?}", t0.elapsed());
    }
    result
}

fn invoke_package_local(
    home: &Path,
    package: &str,
    command_path: &str,
    parsed_flags: &BTreeMap<String, String>,
    capabilities: &CapabilityRegistry,
    depth: usize,
    max_depth: usize,
    package_router: &dyn PackageRouter,
    on_output: &mut dyn FnMut(serde_json::Value) -> std::result::Result<(), String>,
) -> Result<i32> {
    let resolved = resolve(home, package)?;
    let command = find_command(&resolved.definition.commands, command_path)?;
    validate_command_input_mode(command)?;
    let (env_vars, placeholders) = build_invocation_env(home, &resolved, command_path, parsed_flags)?;

    if resolved.definition.wasm.is_some() {
        return execute_wasm_with_handler(
            home,
            &resolved,
            command_path,
            command,
            parsed_flags,
            &env_vars,
            capabilities,
            package_router,
            depth,
            max_depth,
            on_output,
        );
    }

    if resolved.definition.protocol {
        let mode = resolve_mode(&resolved);
        if mode == receipt::Mode::Service && resolved.definition.service.is_some() {
            return execute_service_with_handler(
                home,
                &resolved,
                command_path,
                command,
                parsed_flags,
                capabilities,
                package_router,
                depth,
                max_depth,
                on_output,
            );
        }
        return execute_protocol_with_handler(
            home,
            &resolved,
            command_path,
            command,
            parsed_flags,
            &env_vars,
            capabilities,
            package_router,
            depth,
            max_depth,
            on_output,
        );
    }

    if let Some(ref invoke) = command.invoke {
        return crate::execute::exec_invoke(invoke, &env_vars, &placeholders);
    }

    bail!(
        "cross-package invocation requires a protocol-enabled package, got '{}'",
        resolved.definition.name
    )
}

fn build_invocation_env(
    home: &Path,
    resolved: &ResolvedPackage,
    command_path: &str,
    parsed_flags: &BTreeMap<String, String>,
) -> Result<(BTreeMap<String, String>, BTreeMap<String, String>)> {
    let cwd = std::env::current_dir().ok();
    let project_root = cwd
        .as_ref()
        .and_then(|cwd| paths::discover_project_root(cwd, home));
    let project_config = project_root
        .as_ref()
        .and_then(|root| config::read_project(root).ok());
    let global_config = config::read_global(home).unwrap_or_default();

    Ok(crate::execute::build_env_vars(
        home,
        &resolved.definition.name,
        command_path,
        &resolved.version,
        parsed_flags,
        find_command(&resolved.definition.commands, command_path)?,
        &resolved.receipt,
        &global_config,
        &resolved.definition.config,
        project_root.as_deref(),
        resolved.definition.project_data,
        project_config.as_ref(),
        cwd.as_deref(),
    ))
}

#[allow(clippy::too_many_arguments)]
fn execute_protocol_with_handler(
    home: &Path,
    resolved: &ResolvedPackage,
    command_path: &str,
    command: &CommandDefinition,
    parsed_flags: &BTreeMap<String, String>,
    env_vars: &BTreeMap<String, String>,
    capabilities: &CapabilityRegistry,
    package_router: &dyn PackageRouter,
    depth: usize,
    max_depth: usize,
    on_output: &mut dyn FnMut(serde_json::Value) -> std::result::Result<(), String>,
) -> Result<i32> {
    let (program, args) = resolve_launch_command(home, &resolved.definition, &resolved.version)?;
    let mut child = Command::new(&program)
        .args(&args)
        .envs(env_vars)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn package '{}'", resolved.definition.name))?;

    let child_stdin = child.stdin.take().unwrap();
    let child_stdout = child.stdout.take().unwrap();
    let invoke_msg = Message::Invoke(proto::Invoke::from_str_args(
        command_path,
        parsed_flags,
        command.input.is_some(),
    ));
    let reader = BufReader::new(child_stdout);
    let mut output_handler = CallbackOutputHandler::new(on_output);
    let result = run_protocol_session_with_handler(
        reader,
        child_stdin,
        &invoke_msg,
        true,
        &resolved.definition,
        command,
        capabilities,
        Some(package_router),
        &mut output_handler,
        None,
        depth,
        max_depth,
    );
    let _ = child.wait();
    if let Some(error) = output_handler.take_error() {
        bail!("{}", error);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn execute_service_with_handler(
    home: &Path,
    resolved: &ResolvedPackage,
    command_path: &str,
    command: &CommandDefinition,
    parsed_flags: &BTreeMap<String, String>,
    capabilities: &CapabilityRegistry,
    package_router: &dyn PackageRouter,
    depth: usize,
    max_depth: usize,
    on_output: &mut dyn FnMut(serde_json::Value) -> std::result::Result<(), String>,
) -> Result<i32> {
    let service = resolved.definition.service.as_ref().unwrap();
    let port = service.port.ok_or_else(|| {
        anyhow!(
            "service package '{}' must declare a port in service.port",
            resolved.definition.name
        )
    })?;
    ensure_service_running(home, &resolved.definition.name, port)?;
    let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).with_context(|| {
        format!(
            "failed to connect to service '{}' on port {}",
            resolved.definition.name, port
        )
    })?;
    let reader = BufReader::new(stream.try_clone().context("failed to clone TCP stream")?);
    let invoke_msg = Message::Invoke(proto::Invoke::from_str_args(
        command_path,
        parsed_flags,
        command.input.is_some(),
    ));
    let mut output_handler = CallbackOutputHandler::new(on_output);
    let result = run_protocol_session_with_handler(
        reader,
        stream,
        &invoke_msg,
        true,
        &resolved.definition,
        command,
        capabilities,
        Some(package_router),
        &mut output_handler,
        None,
        depth,
        max_depth,
    );
    if let Some(error) = output_handler.take_error() {
        bail!("{}", error);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn execute_wasm_with_handler(
    home: &Path,
    resolved: &ResolvedPackage,
    command_path: &str,
    command: &CommandDefinition,
    parsed_flags: &BTreeMap<String, String>,
    env_vars: &BTreeMap<String, String>,
    capabilities: &CapabilityRegistry,
    package_router: &dyn PackageRouter,
    depth: usize,
    max_depth: usize,
    on_output: &mut dyn FnMut(serde_json::Value) -> std::result::Result<(), String>,
) -> Result<i32> {
    let wasm_name = resolved.definition.wasm.as_ref().ok_or_else(|| {
        anyhow!(
            "internal error: execute_wasm_with_handler called for non-wasm package '{}'",
            resolved.definition.name
        )
    })?;
    let wasm_path = paths::store_wasm_path(home, &resolved.definition.name, &resolved.version, wasm_name);
    if !wasm_path.exists() {
        return Err(crate::error::DispatchError::ArtifactMissing(wasm_path).into());
    }

    let invoke_msg = Message::Invoke(proto::Invoke::from_str_args(
        command_path,
        parsed_flags,
        command.input.is_some(),
    ));

    if resolved
        .definition
        .service
        .as_ref()
        .is_some_and(|service| service.library)
    {
        match crate::daemon_client::try_open_library_invoke_stream(
            home,
            &resolved.definition.name,
            &resolved.version,
            command_path,
            parsed_flags,
            env_vars,
        ) {
            Ok(Some(stream)) => {
                let tcp_reader = BufReader::new(
                    stream
                        .try_clone()
                        .context("cloning library invoke stream for session read")?,
                );
                let mut output_handler = CallbackOutputHandler::new(on_output);
                let result = run_protocol_session_with_handler(
                    tcp_reader,
                    stream,
                    &invoke_msg,
                    false,
                    &resolved.definition,
                    command,
                    capabilities,
                    Some(package_router),
                    &mut output_handler,
                    None,
                    depth,
                    max_depth,
                );
                if let Some(error) = output_handler.take_error() {
                    bail!("{}", error);
                }
                return result;
            }
            Ok(None) => {}
            Err(error) => {
                eprintln!(
                    "warning: daemon library invoke failed - falling back to fresh instance: {:#}",
                    error
                );
            }
        }
    }

    match crate::daemon_client::try_open_dispatch_stream(
        home,
        &resolved.definition.name,
        &resolved.version,
        env_vars,
    ) {
        Ok(Some(stream)) => {
            let tcp_reader = BufReader::new(stream.try_clone().context("cloning daemon stream for session read")?);
            let mut output_handler = CallbackOutputHandler::new(on_output);
            let result = run_protocol_session_with_handler(
                tcp_reader,
                stream,
                &invoke_msg,
                true,
                &resolved.definition,
                command,
                capabilities,
                Some(package_router),
                &mut output_handler,
                None,
                depth,
                max_depth,
            );
            if let Some(error) = output_handler.take_error() {
                bail!("{}", error);
            }
            return result;
        }
        Ok(None) => {}
        Err(_) => {}
    }

    let host = wasm_runtime::WasmHost::shared()?;
    let module = host.load_module(&wasm_path)?;
    let env: Vec<(String, String)> = env_vars.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let wasm_runtime::WasmSession {
        writer,
        reader,
        controller,
    } = host.invoke(module, env)?;
    let mut output_handler = CallbackOutputHandler::new(on_output);
    let result = run_protocol_session_with_handler(
        reader,
        writer,
        &invoke_msg,
        true,
        &resolved.definition,
        command,
        capabilities,
        Some(package_router),
        &mut output_handler,
        None,
        depth,
        max_depth,
    );
    let _ = controller.finish();
    if let Some(error) = output_handler.take_error() {
        bail!("{}", error);
    }
    result
}

// ─── Legacy Helpers ──────────────────────────────────────────────────

fn execute_command(
    home: &Path,
    resolved: &ResolvedPackage,
    env_vars: &BTreeMap<String, String>,
    placeholders: &BTreeMap<String, String>,
    command: &CommandDefinition,
    output_mode: OutputMode,
) -> Result<i32> {
    let render_mode = resolve_render_mode(
        output_mode,
        &command.output,
        std::io::stdout().is_terminal(),
    );

    if let Some(ref binary_name) = resolved.definition.binary {
        // Binary package: exec with env vars and empty argv
        let bin_path = paths::store_binary_path(
            home,
            &resolved.definition.name,
            &resolved.version,
            binary_name,
        );
        if !bin_path.exists() {
            bail!(
                "binary '{}' not found for '{}' v{}\nhint: reinstall with `zacor install <source>`",
                binary_name,
                resolved.definition.name,
                resolved.version
            );
        }
        let output_decl =
            render_mode.and_then(|mode| command.output.as_ref().map(|output| (output, mode)));
        exec_binary(&bin_path, &resolved.definition.name, env_vars, output_decl)
    } else if let Some(ref invoke) = command.invoke {
        crate::execute::exec_invoke(invoke, env_vars, placeholders)
    } else {
        bail!(
            "package '{}' has no binary and no invoke template for this command",
            resolved.definition.name
        );
    }
}

fn exec_binary(
    bin: &Path,
    name: &str,
    env_vars: &BTreeMap<String, String>,
    output: Option<(&OutputDeclaration, RenderMode)>,
) -> Result<i32> {
    #[cfg(unix)]
    if output.is_none() {
        use std::os::unix::process::CommandExt;
        let err = Command::new(bin)
            .envs(env_vars)
            .stdin(process::Stdio::inherit())
            .stdout(process::Stdio::inherit())
            .stderr(process::Stdio::inherit())
            .exec();
        return Err(anyhow!(err).context(format!("failed to exec package '{}'", name)));
    }

    #[cfg(windows)]
    let _job = match crate::job_object::JobObject::setup() {
        Ok(job) => Some(job),
        Err(e) => {
            eprintln!("warning: failed to create Job Object: {:#}", e);
            None
        }
    };

    let stdout_cfg = if output.is_some() {
        process::Stdio::piped()
    } else {
        process::Stdio::inherit()
    };

    let mut child = Command::new(bin)
        .envs(env_vars)
        .stdin(process::Stdio::inherit())
        .stdout(stdout_cfg)
        .stderr(process::Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to execute package '{}'", name))?;

    #[cfg(windows)]
    if let Some(ref job) = _job
        && let Err(e) = job.assign(&child)
    {
        eprintln!("warning: failed to assign process to Job Object: {:#}", e);
    }

    if let Some((output_decl, render_mode)) = output
        && let Some(child_stdout) = child.stdout.take()
    {
        let reader = BufReader::new(child_stdout);
        let stdout = std::io::stdout();
        let writer = std::io::BufWriter::new(stdout.lock());
        crate::render::render_jsonl(reader, output_decl, render_mode, writer);
    }

    let status = child
        .wait()
        .with_context(|| format!("failed to wait for package '{}'", name))?;
    Ok(status.code().unwrap_or(1))
}

// ─── Public Entry Point ──────────────────────────────────────────────

pub fn run(home: &Path, name: &str, args: &[String], output_mode: OutputMode) -> Result<i32> {
    let resolved = resolve(home, name)?;

    // Build clap command from package definition
    let cmd = build_clap_command(&resolved.definition);

    // Parse with clap
    let (command_path, parsed_flags) = match clap_parse(cmd, name, args, &resolved.definition) {
        Ok(result) => result,
        Err(e) => {
            if e.use_stderr() {
                eprint!("{}", e);
                return Ok(2);
            } else {
                print!("{}", e);
                return Ok(0);
            }
        }
    };

    // Find the command definition
    let command = find_command(&resolved.definition.commands, &command_path)?;
    validate_command_input_mode(command)?;

    // Discover project root
    let cwd = std::env::current_dir().ok();
    let project_root = match cwd {
        Some(ref c) => paths::discover_project_root(c, home),
        None => None,
    };

    // Read project config if available
    let project_config = project_root
        .as_ref()
        .and_then(|root| config::read_project(root).ok());

    // Build env vars and placeholder map
    let global_config = config::read_global(home).unwrap_or_default();
    let (env_vars, placeholders) = crate::execute::build_env_vars(
        home,
        &resolved.definition.name,
        &command_path,
        &resolved.version,
        &parsed_flags,
        command,
        &resolved.receipt,
        &global_config,
        &resolved.definition.config,
        project_root.as_deref(),
        resolved.definition.project_data,
        project_config.as_ref(),
        cwd.as_deref(),
    );
    let capabilities = crate::providers::build_default_registry();
    let package_router = LocalPackageRouter {
        home,
        capabilities: &capabilities,
    };

    // Execute
    execute(
        home,
        &resolved,
        &env_vars,
        &placeholders,
        &command_path,
        command,
        &parsed_flags,
        output_mode,
        &capabilities,
        Some(&package_router),
        0,
        DEFAULT_MAX_CALL_DEPTH,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_definition::{InlineInputFallback, InputType, OutputType};
    use crate::test_util;

    #[test]
    fn test_dispatch_missing_package() {
        let home = test_util::temp_home("dispatch");
        let result = run(home.path(), "nonexistent", &[], OutputMode::Auto);
        assert!(result.is_err());
        let err = format!("{:#}", result.unwrap_err());
        assert!(err.contains("not found"), "got: {}", err);
    }

    #[test]
    fn test_dispatch_disabled_package() {
        let home = test_util::temp_home("dispatch");
        let mut r = receipt::Receipt::new(
            "1.0.0".to_string(),
            receipt::SourceRecord::Local {
                path: "/tmp/mymod".to_string(),
            },
        );
        r.active = false;
        receipt::write(home.path(), "mymod", &r).unwrap();

        let result = run(home.path(), "mymod", &[], OutputMode::Auto);
        assert!(result.is_err());
        let err = format!("{:#}", result.unwrap_err());
        assert!(err.contains("disabled"), "got: {}", err);
        assert!(err.contains("zacor enable"), "got: {}", err);
    }

    #[test]
    fn test_dispatch_corrupt_definition() {
        let home = test_util::temp_home("dispatch");
        receipt::write(
            home.path(),
            "broken",
            &receipt::Receipt::new(
                "1.0.0".to_string(),
                receipt::SourceRecord::Local {
                    path: "/tmp/broken".to_string(),
                },
            ),
        )
        .unwrap();
        // No package.yaml in store
        let result = run(home.path(), "broken", &[], OutputMode::Auto);
        assert!(result.is_err());
        let err = format!("{:#}", result.unwrap_err());
        assert!(
            err.contains("not found in store") || err.contains("reinstall"),
            "got: {}",
            err
        );
    }

    // ─── mode resolution tests ──────────────────────────────────────

    fn make_resolved(
        mode: Option<receipt::Mode>,
        exec_default: Option<&str>,
        service: bool,
    ) -> ResolvedPackage {
        let mut r = receipt::Receipt::new(
            "1.0.0".to_string(),
            receipt::SourceRecord::Local {
                path: "/tmp/test".to_string(),
            },
        );
        r.mode = mode;

        let mut def = crate::package_definition::parse(
            r#"
name: test
version: "1.0.0"
protocol: true
commands:
  default:
    description: test
"#,
        )
        .unwrap();

        if let Some(default) = exec_default {
            def.execution = Some(crate::package_definition::ExecutionSection {
                default: Some(default.to_string()),
            });
        }
        if service {
            def.service = Some(crate::package_definition::ServiceSection {
                start: Some("test".into()),
                port: Some(9999),
                health: None,
                startup: None,
                library: false,
                idle_timeout_secs: None,
                max_concurrent: None,
            });
        }

        ResolvedPackage {
            receipt: r,
            definition: def,
            version: "1.0.0".to_string(),
        }
    }

    #[test]
    fn test_mode_resolution_receipt_overrides_definition() {
        let resolved = make_resolved(Some(receipt::Mode::Service), Some("command"), true);
        assert_eq!(resolve_mode(&resolved), receipt::Mode::Service);
    }

    #[test]
    fn test_mode_resolution_definition_default() {
        let resolved = make_resolved(None, Some("service"), true);
        assert_eq!(resolve_mode(&resolved), receipt::Mode::Service);
    }

    #[test]
    fn test_mode_resolution_fallback_to_command() {
        let resolved = make_resolved(None, None, false);
        assert_eq!(resolve_mode(&resolved), receipt::Mode::Command);
    }

    #[test]
    fn test_mode_resolution_receipt_command_overrides_service_default() {
        let resolved = make_resolved(Some(receipt::Mode::Command), Some("service"), true);
        assert_eq!(resolve_mode(&resolved), receipt::Mode::Command);
    }

    // ─── output mode tests ──────────────────────────────────────────

    #[test]
    fn test_resolve_render_mode() {
        let output = Some(crate::package_definition::OutputDeclaration {
            output_type: Some(OutputType::Table),
            cardinality: None,
            display: None,
            schema: None,
            field: None,
            stream: false,
        });
        assert_eq!(
            resolve_render_mode(OutputMode::Auto, &output, true),
            Some(RenderMode::Rich)
        );
        assert_eq!(resolve_render_mode(OutputMode::Auto, &output, false), None);
        assert_eq!(
            resolve_render_mode(OutputMode::Text, &output, false),
            Some(RenderMode::Rich)
        );
        assert_eq!(
            resolve_render_mode(OutputMode::Plain, &output, false),
            Some(RenderMode::Plain)
        );
        assert_eq!(resolve_render_mode(OutputMode::Json, &output, true), None);
        // no output declaration → no render
        assert_eq!(resolve_render_mode(OutputMode::Plain, &None, false), None);
    }

    #[test]
    fn test_resolve_launch_command_binary() {
        let home = test_util::temp_home("resolve_launch");
        let store = paths::store_path(home.path(), "my-pkg", "1.0.0");
        std::fs::create_dir_all(&store).unwrap();
        let bin_name = format!("my-pkg{}", crate::platform::exe_suffix());
        std::fs::write(store.join(&bin_name), "fake binary").unwrap();

        let def = PackageDefinition {
            name: "my-pkg".into(),
            version: "1.0.0".into(),
            binary: Some("my-pkg".into()),
            run: None,
            wasm: None,
            description: None,
            protocol: true,
            commands: BTreeMap::new(),
            config: BTreeMap::new(),
            depends: Default::default(),
            service: None,
            execution: None,
            build: None,
            project_data: false,
        };

        let (program, args) = resolve_launch_command(home.path(), &def, "1.0.0").unwrap();
        assert!(program.contains("my-pkg"), "got: {}", program);
        assert!(args.is_empty());
    }

    #[test]
    fn test_resolve_launch_command_run() {
        let home = test_util::temp_home("resolve_launch_run");
        let store = paths::store_path(home.path(), "py-pkg", "0.1.0");
        std::fs::create_dir_all(&store).unwrap();
        std::fs::write(store.join("main.py"), "print('hello')").unwrap();

        let def = PackageDefinition {
            name: "py-pkg".into(),
            version: "0.1.0".into(),
            binary: None,
            run: Some("python3 main.py".into()),
            wasm: None,
            description: None,
            protocol: true,
            commands: BTreeMap::new(),
            config: BTreeMap::new(),
            depends: Default::default(),
            service: None,
            execution: None,
            build: None,
            project_data: false,
        };

        let (program, args) = resolve_launch_command(home.path(), &def, "0.1.0").unwrap();
        // python3 is on PATH, not resolved to store
        assert_eq!(program, "python3");
        // main.py exists in store, should be resolved to full path
        assert_eq!(args.len(), 1);
        assert!(args[0].contains("main.py"), "got: {}", args[0]);
        assert!(
            args[0].contains("py-pkg"),
            "should be full store path, got: {}",
            args[0]
        );
    }

    #[test]
    fn test_resolve_launch_command_neither() {
        let home = test_util::temp_home("resolve_launch_neither");
        let def = PackageDefinition {
            name: "bad-pkg".into(),
            version: "1.0.0".into(),
            binary: None,
            run: None,
            wasm: None,
            description: None,
            protocol: true,
            commands: BTreeMap::new(),
            config: BTreeMap::new(),
            depends: Default::default(),
            service: None,
            execution: None,
            build: None,
            project_data: false,
        };

        let err = resolve_launch_command(home.path(), &def, "1.0.0").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("run") && msg.contains("binary"),
            "got: {}",
            msg
        );
    }

    #[test]
    fn test_validate_command_input_mode_rejects_interactive_stdin_without_fallback() {
        let command = CommandDefinition {
            input: Some(InputType::Jsonl),
            ..Default::default()
        };

        let err = validate_command_input_mode_for_stdin(&command, true).unwrap_err();
        assert!(err.to_string().contains("requires piped stdin"));
    }

    #[test]
    fn test_validate_command_input_mode_allows_interactive_stdin_with_fallback() {
        let command = CommandDefinition {
            input: Some(InputType::Jsonl),
            inline_input_fallback: Some(InlineInputFallback::StringValue),
            ..Default::default()
        };

        validate_command_input_mode_for_stdin(&command, true).unwrap();
    }
}
