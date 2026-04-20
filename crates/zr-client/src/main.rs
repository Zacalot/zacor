//! Thin `zr` client — minimal dispatch frontend to the zacor daemon.
//!
//! Scope is deliberately tiny: connect, send a dispatch request, run
//! the session loop over the TCP stream, render OUTPUT records, write
//! CAPABILITY_RES back when needed. No wasmtime, no tokio, no axum, no
//! HTTP — just std + serde_json. The resulting binary starts ~10×
//! faster than the fat `zr` built from the zacor crate.
//!
//! Capabilities supported: `fs.*` served locally against the user's
//! real cwd. `prompt.*` and `clipboard.*` return "unsupported" under
//! the thin client (v1 limitation — re-add later with minimal deps).

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::exit;

const DAEMON_ADDR: &str = "127.0.0.1:19100";

fn main() {
    let argv: Vec<String> = env::args().skip(1).collect();
    if argv.is_empty() {
        eprintln!("usage: zr-client <package> [args...]");
        exit(2);
    }

    match run(&argv) {
        Ok(code) => exit(code),
        Err(e) => {
            eprintln!("zr-client: {}", e);
            exit(1);
        }
    }
}

fn run(argv: &[String]) -> Result<i32, String> {
    let pkg_name = argv[0].clone();
    let rest_args: Vec<String> = argv[1..].to_vec();

    let home = resolve_zr_home()?;
    let (version, definition) = read_package(&home, &pkg_name)?;

    // Primitive arg packing: first positional goes to the first declared
    // arg of the default command. Good enough for echo/cat demos; a real
    // thin client would mirror clap_builder's logic.
    let (command_name, args_map) = pack_args(&definition, &rest_args)?;
    let env_map = build_basic_env(&home, &pkg_name, &version, &command_name);

    let stream = open_dispatch(&pkg_name, &version, &env_map)?;
    run_session(stream, &command_name, &args_map)
}

fn resolve_zr_home() -> Result<PathBuf, String> {
    if let Ok(h) = env::var("ZR_HOME") {
        return Ok(PathBuf::from(h));
    }
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .map_err(|_| "could not determine home directory — set ZR_HOME".to_string())?;
    Ok(PathBuf::from(home).join(".zr"))
}

fn read_package(home: &Path, pkg: &str) -> Result<(String, serde_json::Value), String> {
    // Receipt → current version.
    let receipt_path = home.join("modules").join(format!("{}.json", pkg));
    let receipt_bytes = fs::read(&receipt_path).map_err(|e| {
        format!(
            "package '{}' not found (no receipt at {}): {}\nhint: install it with `zacor install <source>`",
            pkg,
            receipt_path.display(),
            e
        )
    })?;
    let receipt: serde_json::Value = serde_json::from_slice(&receipt_bytes)
        .map_err(|e| format!("parse receipt {}: {}", receipt_path.display(), e))?;

    if !receipt
        .get("active")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Err(format!("package '{}' is disabled", pkg));
    }

    let version = receipt
        .get("current")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "receipt missing `current`".to_string())?
        .to_string();

    // Pull the embedded manifest from the wasm file — this is the
    // source of truth for the command schema. A thin client extracts
    // the `zacor_manifest` custom section by scanning wasm sections
    // manually (no wasmparser dep).
    let store_dir = home.join("store").join(pkg).join(&version);
    let wasm_path = find_wasm_in_dir(&store_dir)?;
    let wasm_bytes = fs::read(&wasm_path)
        .map_err(|e| format!("read wasm {}: {}", wasm_path.display(), e))?;
    let manifest_yaml = extract_custom_section(&wasm_bytes, "zacor_manifest").ok_or_else(|| {
        format!(
            "wasm at {} has no embedded zacor_manifest section",
            wasm_path.display()
        )
    })?;

    // We don't need full yaml parsing — a thin client just needs the
    // command list and their first-arg names to pack argv. Parse
    // minimal YAML by hand (cheap) or defer to a tiny yaml crate.
    // For v1 we rely on serde_yml via serde_json's JSON parser — since
    // the generated yaml is flat and simple. Actually the embedded
    // manifest is yaml, not JSON. Punting: use naive line-based
    // parsing sufficient for the v1 cases.
    let manifest_str = std::str::from_utf8(&manifest_yaml)
        .map_err(|e| format!("manifest is not utf-8: {}", e))?;
    let definition = parse_minimal_manifest(manifest_str).map_err(|e| {
        format!(
            "parse embedded manifest for '{}' v{}: {}",
            pkg, version, e
        )
    })?;

    Ok((version, definition))
}

fn find_wasm_in_dir(dir: &Path) -> Result<PathBuf, String> {
    let entries =
        fs::read_dir(dir).map_err(|e| format!("reading store dir {}: {}", dir.display(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("reading store entry: {}", e))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
            return Ok(path);
        }
    }
    Err(format!("no .wasm artifact in {}", dir.display()))
}

/// Walk wasm custom sections (section id 0) looking for a named section.
/// See WebAssembly binary format spec.
fn extract_custom_section(wasm: &[u8], name: &str) -> Option<Vec<u8>> {
    if wasm.len() < 8 || &wasm[0..4] != b"\0asm" {
        return None;
    }
    let mut pos = 8; // skip magic + version
    while pos < wasm.len() {
        let section_id = wasm[pos];
        pos += 1;
        let (size, consumed) = read_leb128(&wasm[pos..])?;
        pos += consumed;
        let section_end = pos + size as usize;
        if section_end > wasm.len() {
            return None;
        }
        if section_id == 0 {
            // Custom section: name_len, name bytes, data
            let (name_len, nc) = read_leb128(&wasm[pos..section_end])?;
            let name_start = pos + nc;
            let name_end = name_start + name_len as usize;
            if name_end > section_end {
                return None;
            }
            let section_name = std::str::from_utf8(&wasm[name_start..name_end]).ok()?;
            if section_name == name {
                return Some(wasm[name_end..section_end].to_vec());
            }
        }
        pos = section_end;
    }
    None
}

fn read_leb128(buf: &[u8]) -> Option<(u32, usize)> {
    let mut result: u32 = 0;
    let mut shift = 0;
    for (i, &b) in buf.iter().enumerate() {
        if shift >= 32 {
            return None;
        }
        result |= ((b & 0x7f) as u32) << shift;
        if b & 0x80 == 0 {
            return Some((result, i + 1));
        }
        shift += 7;
    }
    None
}

/// Naive parser: pulls out the list of top-level commands, their args,
/// and whether each arg has a `flag:` or is positional. Sufficient for
/// a thin client to pack argv into an invoke args map for simple
/// packages. Complex packages (nested subcommands, type constraints,
/// etc.) should use the fat zr.
fn parse_minimal_manifest(yaml: &str) -> Result<serde_json::Value, String> {
    // Walk lines, tracking indentation to find commands → <name> → args → <arg> → ...
    let mut commands: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    let mut in_commands = false;
    let mut current_cmd: Option<String> = None;
    let mut current_cmd_obj: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    let mut in_args = false;
    let mut current_arg: Option<String> = None;
    let mut current_arg_obj: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    let mut args_obj: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    let mut arg_order: Vec<String> = Vec::new();

    let flush_arg =
        |arg: &mut Option<String>,
         arg_obj: &mut serde_json::Map<String, serde_json::Value>,
         args: &mut serde_json::Map<String, serde_json::Value>,
         order: &mut Vec<String>| {
            if let Some(n) = arg.take() {
                order.push(n.clone());
                args.insert(n, serde_json::Value::Object(std::mem::take(arg_obj)));
            }
        };

    let flush_cmd =
        |cmd: &mut Option<String>,
         cmd_obj: &mut serde_json::Map<String, serde_json::Value>,
         args: &mut serde_json::Map<String, serde_json::Value>,
         order: &mut Vec<String>,
         cmds: &mut serde_json::Map<String, serde_json::Value>| {
            if let Some(n) = cmd.take() {
                if !args.is_empty() {
                    cmd_obj.insert("args".into(), serde_json::Value::Object(std::mem::take(args)));
                    cmd_obj.insert(
                        "arg_order".into(),
                        serde_json::Value::Array(
                            std::mem::take(order)
                                .into_iter()
                                .map(serde_json::Value::String)
                                .collect(),
                        ),
                    );
                }
                cmds.insert(n, serde_json::Value::Object(std::mem::take(cmd_obj)));
            }
        };

    for line in yaml.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();

        if indent == 0 {
            // Top-level — e.g., `commands:`, `name:`, `version:`
            in_commands = trimmed == "commands:";
            flush_arg(
                &mut current_arg,
                &mut current_arg_obj,
                &mut args_obj,
                &mut arg_order,
            );
            flush_cmd(
                &mut current_cmd,
                &mut current_cmd_obj,
                &mut args_obj,
                &mut arg_order,
                &mut commands,
            );
            in_args = false;
        } else if in_commands && indent == 2 && trimmed.ends_with(':') {
            // A command name like `  default:`
            flush_arg(
                &mut current_arg,
                &mut current_arg_obj,
                &mut args_obj,
                &mut arg_order,
            );
            flush_cmd(
                &mut current_cmd,
                &mut current_cmd_obj,
                &mut args_obj,
                &mut arg_order,
                &mut commands,
            );
            current_cmd = Some(trimmed.trim_end_matches(':').to_string());
            in_args = false;
        } else if in_commands && indent == 4 && trimmed == "args:" {
            in_args = true;
        } else if in_commands && indent == 4 && !trimmed.starts_with('-') {
            // Other command field (description, input, output) — ignore for thin parsing
            in_args = false;
        } else if in_commands && in_args && indent == 6 && trimmed.ends_with(':') {
            // An arg name like `      text:`
            flush_arg(
                &mut current_arg,
                &mut current_arg_obj,
                &mut args_obj,
                &mut arg_order,
            );
            current_arg = Some(trimmed.trim_end_matches(':').to_string());
        } else if in_commands && in_args && indent == 8 {
            // An arg property like `        type: string`
            if let Some((k, v)) = trimmed.split_once(':') {
                let v = v.trim().trim_matches('"').to_string();
                current_arg_obj.insert(
                    k.trim().to_string(),
                    serde_json::Value::String(v),
                );
            }
        }
    }

    flush_arg(
        &mut current_arg,
        &mut current_arg_obj,
        &mut args_obj,
        &mut arg_order,
    );
    flush_cmd(
        &mut current_cmd,
        &mut current_cmd_obj,
        &mut args_obj,
        &mut arg_order,
        &mut commands,
    );

    Ok(serde_json::json!({"commands": commands}))
}

fn pack_args(
    definition: &serde_json::Value,
    rest: &[String],
) -> Result<(String, BTreeMap<String, serde_json::Value>), String> {
    let commands = definition["commands"]
        .as_object()
        .ok_or_else(|| "manifest has no commands".to_string())?;

    // Determine command name + starting argv position for command args.
    // If `default` exists and no positional arg matches a subcommand name,
    // use `default`. Otherwise the first positional is the command name.
    let (command_name, arg_argv) = if let Some(first) = rest.first() {
        if commands.contains_key(first) && first != "default" {
            (first.clone(), &rest[1..])
        } else if commands.contains_key("default") {
            ("default".to_string(), &rest[..])
        } else {
            return Err(format!("no default command; pass a subcommand"));
        }
    } else if commands.contains_key("default") {
        ("default".to_string(), &rest[..])
    } else {
        return Err("no default command; pass a subcommand".to_string());
    };

    let cmd = &commands[&command_name];
    let arg_order: Vec<String> = cmd["arg_order"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let args_schema = cmd["args"].as_object();

    let mut args_map: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    // Simple positional packing — map argv[i] to arg_order[i]. A real
    // client would parse --flag-style args; thin client keeps it simple.
    for (i, v) in arg_argv.iter().enumerate() {
        if let Some(name) = arg_order.get(i) {
            // Join remaining argv if this is the last arg and we want
            // to capture trailing spaces (as echo does with `text`)
            let value = if i == arg_order.len() - 1 {
                arg_argv[i..].join(" ")
            } else {
                v.clone()
            };

            let typed = coerce_arg_type(args_schema, name, &value);
            args_map.insert(name.clone(), typed);
            if i == arg_order.len() - 1 {
                break;
            }
        } else {
            // Extra argv beyond schema — ignore for now.
            break;
        }
    }

    Ok((command_name, args_map))
}

fn coerce_arg_type(
    args_schema: Option<&serde_json::Map<String, serde_json::Value>>,
    name: &str,
    raw: &str,
) -> serde_json::Value {
    let t = args_schema
        .and_then(|a| a.get(name))
        .and_then(|v| v.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("string");
    match t {
        "integer" | "number" => {
            if let Ok(n) = raw.parse::<i64>() {
                return serde_json::Value::Number(n.into());
            }
            if let Ok(f) = raw.parse::<f64>() {
                if let Some(n) = serde_json::Number::from_f64(f) {
                    return serde_json::Value::Number(n);
                }
            }
            serde_json::Value::String(raw.to_string())
        }
        "bool" => serde_json::Value::Bool(matches!(raw, "true" | "1" | "yes")),
        _ => serde_json::Value::String(raw.to_string()),
    }
}

fn build_basic_env(
    home: &Path,
    pkg: &str,
    version: &str,
    command: &str,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert("ZR_PACKAGE".into(), pkg.to_string());
    env.insert("ZR_COMMAND".into(), command.to_string());
    env.insert("ZR_VERSION".into(), version.to_string());
    env.insert("ZR_HOME".into(), home.display().to_string());
    env
}

fn open_dispatch(
    pkg: &str,
    version: &str,
    env: &BTreeMap<String, String>,
) -> Result<TcpStream, String> {
    let mut stream = TcpStream::connect(DAEMON_ADDR)
        .map_err(|e| format!("connect to daemon at {}: {}\nhint: run `zacor daemon start`", DAEMON_ADDR, e))?;

    let req = serde_json::json!({
        "request": "dispatch",
        "pkg_name": pkg,
        "version": version,
        "env": env,
    });
    writeln!(stream, "{}", req).map_err(|e| format!("write dispatch request: {}", e))?;
    stream.flush().ok();

    // Read ack byte-by-byte to avoid buffering past the newline.
    let mut ack = String::new();
    let mut buf = [0u8; 1];
    loop {
        let n = stream.read(&mut buf).map_err(|e| format!("read ack: {}", e))?;
        if n == 0 || buf[0] == b'\n' {
            break;
        }
        ack.push(buf[0] as char);
    }
    let ack_val: serde_json::Value =
        serde_json::from_str(ack.trim()).map_err(|e| format!("parse ack: {}", e))?;
    if !ack_val["ok"].as_bool().unwrap_or(false) {
        let err = ack_val["error"].as_str().unwrap_or("unknown");
        return Err(format!("daemon rejected dispatch: {}", err));
    }
    Ok(stream)
}

fn run_session(
    mut stream: TcpStream,
    command: &str,
    args: &BTreeMap<String, serde_json::Value>,
) -> Result<i32, String> {
    // Send INVOKE.
    let invoke = serde_json::json!({
        "type": "invoke",
        "command": command,
        "args": args,
    });
    writeln!(stream, "{}", invoke).map_err(|e| format!("write invoke: {}", e))?;
    stream.flush().ok();

    let reader_stream = stream
        .try_clone()
        .map_err(|e| format!("clone stream: {}", e))?;
    let mut reader = BufReader::new(reader_stream);

    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();

    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| format!("read frame: {}", e))?;
        if n == 0 {
            return Ok(1);
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            continue;
        }
        let frame: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue, // forward-compat: ignore unparseable
        };
        match frame["type"].as_str().unwrap_or("") {
            "output" => {
                // Thin renderer: print the record's JSON (one per line) —
                // ~ what fat zr does in non-TTY auto mode.
                if let Some(record) = frame.get("record") {
                    writeln!(stdout, "{}", record).ok();
                }
            }
            "capability_req" => {
                let id = frame["id"].as_u64().unwrap_or(0);
                let domain = frame["domain"].as_str().unwrap_or("");
                let op = frame["op"].as_str().unwrap_or("");
                let params = frame.get("params").cloned().unwrap_or(serde_json::json!({}));
                let res = handle_capability(id, domain, op, &params);
                writeln!(stream, "{}", res).map_err(|e| format!("write cap_res: {}", e))?;
                stream.flush().ok();
            }
            "progress" => {
                // Thin client ignores progress.
            }
            "done" => {
                let exit_code = frame["exit_code"].as_i64().unwrap_or(1) as i32;
                if let Some(err) = frame.get("error").and_then(|v| v.as_str()) {
                    eprintln!("error: {}", err);
                }
                return Ok(exit_code);
            }
            _ => {}
        }
    }
}

fn handle_capability(
    id: u64,
    domain: &str,
    op: &str,
    params: &serde_json::Value,
) -> serde_json::Value {
    if domain != "fs" {
        return serde_json::json!({
            "type": "capability_res",
            "id": id,
            "status": "error",
            "error": {"kind": "unsupported", "message": format!("thin zr-client does not implement {}/{}", domain, op)},
        });
    }
    let path = params["path"].as_str().unwrap_or("");
    let result = match op {
        "read_string" => fs::read_to_string(path)
            .map(|s| serde_json::json!({"content": s}))
            .map_err(|e| e.to_string()),
        "read" => fs::read(path)
            .map(|b| {
                let b64 = base64_encode(&b);
                serde_json::json!({"content": b64})
            })
            .map_err(|e| e.to_string()),
        "exists" => Ok(serde_json::json!({"exists": Path::new(path).exists()})),
        "stat" => stat(path),
        "read_dir" => read_dir(path),
        "write" => write(path, params),
        "create_dir_all" => fs::create_dir_all(path)
            .map(|_| serde_json::json!({}))
            .map_err(|e| e.to_string()),
        "rename" => {
            let from = params["from"].as_str().unwrap_or("");
            let to = params["to"].as_str().unwrap_or("");
            fs::rename(from, to)
                .map(|_| serde_json::json!({}))
                .map_err(|e| e.to_string())
        }
        "walk" => walk(path, params),
        _ => Err(format!("unsupported fs op: {}", op)),
    };
    match result {
        Ok(data) => serde_json::json!({
            "type": "capability_res",
            "id": id,
            "status": "ok",
            "data": data,
        }),
        Err(e) => serde_json::json!({
            "type": "capability_res",
            "id": id,
            "status": "error",
            "error": {"kind": "other", "message": e},
        }),
    }
}

fn stat(path: &str) -> Result<serde_json::Value, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    let ft = meta.file_type();
    let file_type = if ft.is_file() {
        "file"
    } else if ft.is_dir() {
        "dir"
    } else if ft.is_symlink() {
        "symlink"
    } else {
        "other"
    };
    let mut result = serde_json::json!({"size": meta.len(), "file_type": file_type});
    if let Ok(modified) = meta.modified()
        && let Ok(epoch) = modified.duration_since(std::time::UNIX_EPOCH)
    {
        result["modified"] = serde_json::json!(epoch.as_secs_f64());
    }
    if let Ok(created) = meta.created()
        && let Ok(epoch) = created.duration_since(std::time::UNIX_EPOCH)
    {
        result["created"] = serde_json::json!(epoch.as_secs_f64());
    }
    Ok(result)
}

fn read_dir(path: &str) -> Result<serde_json::Value, String> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        let ft = meta.file_type();
        let file_type = if ft.is_file() {
            "file"
        } else if ft.is_dir() {
            "dir"
        } else if ft.is_symlink() {
            "symlink"
        } else {
            "other"
        };
        let mut e = serde_json::json!({
            "name": entry.file_name().to_string_lossy(),
            "size": meta.len(),
            "file_type": file_type,
        });
        if let Ok(modified) = meta.modified()
            && let Ok(epoch) = modified.duration_since(std::time::UNIX_EPOCH)
        {
            e["modified"] = serde_json::json!(epoch.as_secs_f64());
        }
        entries.push(e);
    }
    Ok(serde_json::json!({"entries": entries}))
}

fn write(path: &str, params: &serde_json::Value) -> Result<serde_json::Value, String> {
    let content = params["content"].as_str().unwrap_or("");
    let decoded = base64_decode(content).map_err(|e| e.to_string())?;
    fs::write(path, decoded).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({}))
}

/// Gitignore-aware recursive walk for the `fs.walk` capability. Mirrors
/// the fat zr's `handle_fs` so tree and other walk-using packages behave
/// identically under thin-client dispatch.
fn walk(path: &str, params: &serde_json::Value) -> Result<serde_json::Value, String> {
    let hidden = params
        .get("hidden")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let gitignore = params
        .get("gitignore")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let max_depth = params
        .get("max_depth")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize);

    let root = Path::new(path);

    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(hidden)
        .git_ignore(gitignore)
        .git_global(gitignore)
        .git_exclude(gitignore)
        .ignore(gitignore)
        .parents(gitignore);
    if let Some(d) = max_depth {
        builder.max_depth(Some(d));
    }

    let mut entries = Vec::new();
    for entry in builder.build().flatten() {
        let p = entry.path();
        if p == root {
            continue;
        }
        let Ok(rel) = p.strip_prefix(root) else { continue };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let is_dir = entry
            .file_type()
            .map(|ft| ft.is_dir())
            .unwrap_or(false);
        entries.push(serde_json::json!({"path": rel_str, "is_dir": is_dir}));
    }

    Ok(serde_json::json!({"entries": entries}))
}

/// Minimal RFC 4648 base64 encoder (no deps).
fn base64_encode(input: &[u8]) -> String {
    const ALPHA: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        out.push(ALPHA[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHA[((n >> 12) & 0x3f) as usize] as char);
        out.push(ALPHA[((n >> 6) & 0x3f) as usize] as char);
        out.push(ALPHA[(n & 0x3f) as usize] as char);
        i += 3;
    }
    match input.len() - i {
        1 => {
            let n = (input[i] as u32) << 16;
            out.push(ALPHA[((n >> 18) & 0x3f) as usize] as char);
            out.push(ALPHA[((n >> 12) & 0x3f) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
            out.push(ALPHA[((n >> 18) & 0x3f) as usize] as char);
            out.push(ALPHA[((n >> 12) & 0x3f) as usize] as char);
            out.push(ALPHA[((n >> 6) & 0x3f) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

/// Minimal RFC 4648 base64 decoder (no deps). Skips whitespace and `=`.
fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let decode_char = |c: u8| -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for c in input.bytes() {
        if c.is_ascii_whitespace() || c == b'=' {
            continue;
        }
        let v = decode_char(c).ok_or_else(|| format!("invalid base64 char: {c:?}"))?;
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    Ok(out)
}
