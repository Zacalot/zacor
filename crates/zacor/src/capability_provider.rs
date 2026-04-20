//! Capability provider — handles CAPABILITY_REQ from modules by executing
//! operations locally. Used by the protocol dispatcher (CLI transport) and
//! the HTTP server.

use serde_json::json;
use std::path::Path;
use zacor_package::io::fs::FileType;
use zacor_package::protocol::{
    self, CapabilityError, CapabilityReq, CapabilityRes, CapabilityResult,
};

/// Handle a CAPABILITY_REQ and return the corresponding CAPABILITY_RES.
pub fn handle(req: &CapabilityReq) -> CapabilityRes {
    let result = match req.domain.as_str() {
        "fs" => handle_fs(&req.op, &req.params),
        "clipboard" => handle_clipboard(&req.op, &req.params),
        "prompt" => handle_prompt(&req.op, &req.params),
        "http" => handle_http(&req.op, &req.params),
        "parse" => handle_parse(&req.op, &req.params),
        "render" => handle_render(&req.op, &req.params),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unknown capability domain: {}", req.domain),
        )),
    };

    CapabilityRes {
        id: req.id,
        result: match result {
            Ok(data) => CapabilityResult::Ok { data },
            Err(e) => CapabilityResult::Error {
                error: CapabilityError::from_io(&e),
            },
        },
    }
}

// ─── Filesystem ──────────────────────────────────────────────────────

fn resolve_path(path_str: &str) -> String {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let resolved = protocol::resolve_path(path_str, &cwd);
    resolved.replace('/', std::path::MAIN_SEPARATOR_STR)
}

fn handle_fs(op: &str, params: &serde_json::Value) -> std::io::Result<serde_json::Value> {
    let path_str = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let resolved = resolve_path(path_str);

    match op {
        "read_string" => {
            let content = std::fs::read_to_string(&resolved)?;
            Ok(json!({"content": content}))
        }
        "read" => {
            let content = std::fs::read(&resolved)?;
            let encoded = protocol::base64_encode(&content);
            Ok(json!({"content": encoded}))
        }
        "read_dir" => {
            let mut entries = Vec::new();
            for entry in std::fs::read_dir(&resolved)? {
                let entry = entry?;
                let meta = entry.metadata()?;
                let ft: FileType = meta.file_type().into();
                let mut e = json!({
                    "name": entry.file_name().to_string_lossy(),
                    "size": meta.len(),
                    "file_type": ft,
                });
                if let Ok(modified) = meta.modified() {
                    if let Ok(epoch) = modified.duration_since(std::time::UNIX_EPOCH) {
                        e["modified"] = json!(epoch.as_secs_f64());
                    }
                }
                entries.push(e);
            }
            Ok(json!({"entries": entries}))
        }
        "stat" => {
            let meta = std::fs::metadata(&resolved)?;
            let ft: FileType = meta.file_type().into();
            let mut result = json!({"size": meta.len(), "file_type": ft});
            if let Ok(modified) = meta.modified() {
                if let Ok(epoch) = modified.duration_since(std::time::UNIX_EPOCH) {
                    result["modified"] = json!(epoch.as_secs_f64());
                }
            }
            if let Ok(created) = meta.created() {
                if let Ok(epoch) = created.duration_since(std::time::UNIX_EPOCH) {
                    result["created"] = json!(epoch.as_secs_f64());
                }
            }
            Ok(result)
        }
        "exists" => Ok(json!({"exists": Path::new(&resolved).exists()})),
        "walk" => fs_walk(&resolved, params),
        "write" => {
            let content = params.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let decoded = protocol::base64_decode(content)?;
            std::fs::write(&resolved, decoded)?;
            Ok(json!({}))
        }
        "create_dir_all" => {
            std::fs::create_dir_all(&resolved)?;
            Ok(json!({}))
        }
        "rename" => {
            let from_str = params.get("from").and_then(|v| v.as_str()).unwrap_or("");
            let to_str = params.get("to").and_then(|v| v.as_str()).unwrap_or("");
            let from_resolved = resolve_path(from_str);
            let to_resolved = resolve_path(to_str);
            std::fs::rename(&from_resolved, &to_resolved)?;
            Ok(json!({}))
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unknown fs operation: {}", op),
        )),
    }
}

/// Recursive directory walk honoring `.gitignore` + hidden filters.
/// Uses the `ignore` crate on the host side so the same rules apply to
/// native subprocess dispatch and wasm dispatch through the capability.
fn fs_walk(resolved: &str, params: &serde_json::Value) -> std::io::Result<serde_json::Value> {
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

    let root = Path::new(resolved);

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
        entries.push(json!({"path": rel_str, "is_dir": is_dir}));
    }

    Ok(json!({"entries": entries}))
}

// ─── Clipboard ───────────────────────────────────────────────────────

fn handle_clipboard(op: &str, params: &serde_json::Value) -> std::io::Result<serde_json::Value> {
    match op {
        "read" => {
            let mut clipboard =
                arboard::Clipboard::new().map_err(|e| std::io::Error::other(e.to_string()))?;
            let text = clipboard
                .get_text()
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            Ok(json!({"text": text}))
        }
        "write" => {
            let text = params.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let mut clipboard =
                arboard::Clipboard::new().map_err(|e| std::io::Error::other(e.to_string()))?;
            clipboard
                .set_text(text.to_string())
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            Ok(json!({}))
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unknown clipboard operation: {}", op),
        )),
    }
}

// ─── Prompt ──────────────────────────────────────────────────────────

fn handle_prompt(op: &str, params: &serde_json::Value) -> std::io::Result<serde_json::Value> {
    use std::io::{IsTerminal, Write};

    if !std::io::stdin().is_terminal() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "prompt not available in piped mode",
        ));
    }

    let message = params.get("message").and_then(|v| v.as_str()).unwrap_or("");

    match op {
        "confirm" => {
            eprint!("{} [y/N] ", message);
            std::io::stderr().flush()?;
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            let answer = matches!(line.trim().to_lowercase().as_str(), "y" | "yes");
            Ok(json!({"answer": answer}))
        }
        "choose" => {
            let options = params
                .get("options")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            eprintln!("{}", message);
            for (i, opt) in options.iter().enumerate() {
                eprintln!("  {}) {}", i + 1, opt);
            }
            eprint!("Choice: ");
            std::io::stderr().flush()?;

            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            let choice = line.trim();

            // Try numeric selection first
            if let Ok(n) = choice.parse::<usize>() {
                if n >= 1 && n <= options.len() {
                    return Ok(json!({"answer": options[n - 1]}));
                }
            }
            // Try exact text match
            if options.iter().any(|o| o == choice) {
                return Ok(json!({"answer": choice}));
            }

            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("invalid choice: {}", choice),
            ))
        }
        "text" => {
            eprint!("{}: ", message);
            std::io::stderr().flush()?;
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            Ok(json!({"answer": line.trim_end()}))
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unknown prompt operation: {}", op),
        )),
    }
}

// ─── HTTP ────────────────────────────────────────────────────────────

fn handle_http(op: &str, params: &serde_json::Value) -> std::io::Result<serde_json::Value> {
    match op {
        "fetch" => http_fetch(params),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unknown http operation: {}", op),
        )),
    }
}

fn http_fetch(params: &serde_json::Value) -> std::io::Result<serde_json::Value> {
    let url = params
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "http.fetch: url is required"))?;

    let method = params
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("GET")
        .to_ascii_uppercase();

    let timeout_ms = params
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(30_000);

    let body_bytes = match params.get("body").and_then(|v| v.as_str()) {
        Some(b64) if !b64.is_empty() => Some(protocol::base64_decode(b64)?),
        _ => None,
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(std::io::Error::other)?;

    let reqwest_method: reqwest::Method = method
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("invalid method '{}': {}", method, e)))?;

    let mut builder = client.request(reqwest_method, url);

    if let Some(headers) = params.get("headers").and_then(|v| v.as_object()) {
        for (name, value) in headers {
            if let Some(v) = value.as_str() {
                builder = builder.header(name, v);
            }
        }
    }

    if let Some(body) = body_bytes {
        builder = builder.body(body);
    }

    let start = std::time::Instant::now();
    let response = builder.send().map_err(std::io::Error::other)?;
    let elapsed_ms = start.elapsed().as_millis() as u64;

    let status = response.status().as_u16();
    let final_url = response.url().to_string();
    let mut headers_map = serde_json::Map::new();
    for (name, value) in response.headers().iter() {
        if let Ok(v) = value.to_str() {
            headers_map.insert(name.as_str().to_string(), json!(v));
        }
    }

    let body = response.bytes().map_err(std::io::Error::other)?;
    let body_b64 = protocol::base64_encode(&body);

    Ok(json!({
        "status": status,
        "headers": headers_map,
        "body": body_b64,
        "final_url": final_url,
        "elapsed_ms": elapsed_ms,
    }))
}

// ─── Parse (tree-sitter) ─────────────────────────────────────────────

fn handle_parse(op: &str, params: &serde_json::Value) -> std::io::Result<serde_json::Value> {
    match op {
        "tree-sitter" => parse_tree_sitter(params),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unknown parse operation: {}", op),
        )),
    }
}

fn parse_language_for_extension(ext: &str) -> Option<tree_sitter::Language> {
    match ext {
        "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
        "ts" | "tsx" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "js" | "jsx" | "mjs" | "cjs" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "py" => Some(tree_sitter_python::LANGUAGE.into()),
        _ => None,
    }
}

fn parse_tree_sitter(params: &serde_json::Value) -> std::io::Result<serde_json::Value> {
    let source = params
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let ext = params.get("ext").and_then(|v| v.as_str()).unwrap_or("");
    let rel_path = params
        .get("rel_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let Some(language) = parse_language_for_extension(ext) else {
        return Ok(json!({"declarations": []}));
    };

    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return Ok(json!({"declarations": []}));
    }

    let Some(tree) = parser.parse(source, None) else {
        return Ok(json!({"declarations": []}));
    };

    let mut decls = Vec::new();
    collect_declarations(tree.root_node(), source, rel_path, ext, &mut decls);
    Ok(json!({"declarations": decls}))
}

fn collect_declarations(
    node: tree_sitter::Node,
    source: &str,
    file: &str,
    ext: &str,
    out: &mut Vec<serde_json::Value>,
) {
    match ext {
        "rs" => collect_rust(node, source, file, out),
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => collect_js_ts(node, source, file, out),
        "py" => collect_python(node, source, file, out),
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_declarations(child, source, file, ext, out);
    }
}

fn decl(file: &str, kind: &str, name: &str, signature: &str) -> serde_json::Value {
    json!({
        "file": file,
        "kind": kind,
        "name": name,
        "signature": signature,
    })
}

fn child_name(node: tree_sitter::Node, source: &str) -> Option<String> {
    node.child_by_field_name("name").map(|n| node_text(n, source))
}

fn node_text(node: tree_sitter::Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").to_string()
}

fn collect_rust(node: tree_sitter::Node, source: &str, file: &str, out: &mut Vec<serde_json::Value>) {
    match node.kind() {
        "function_item" => {
            if let Some(name) = child_name(node, source) {
                let sig = first_line(&node_text(node, source));
                out.push(decl(file, "function", &name, &sig));
            }
        }
        "struct_item" => {
            if let Some(name) = child_name(node, source) {
                out.push(decl(file, "struct", &name, &format!("struct {name}")));
            }
        }
        "enum_item" => {
            if let Some(name) = child_name(node, source) {
                out.push(decl(file, "enum", &name, &format!("enum {name}")));
            }
        }
        "trait_item" => {
            if let Some(name) = child_name(node, source) {
                out.push(decl(file, "trait", &name, &format!("trait {name}")));
            }
        }
        "impl_item" => {
            let text = first_line(&node_text(node, source));
            let name = text.trim_start_matches("impl ").to_string();
            out.push(decl(file, "impl", &name, &text));
        }
        "type_item" => {
            if let Some(name) = child_name(node, source) {
                let sig = first_line(&node_text(node, source));
                out.push(decl(file, "type", &name, &sig));
            }
        }
        _ => {}
    }
}

fn collect_js_ts(node: tree_sitter::Node, source: &str, file: &str, out: &mut Vec<serde_json::Value>) {
    match node.kind() {
        "function_declaration" => {
            if let Some(name) = child_name(node, source) {
                let sig = first_line(&node_text(node, source));
                out.push(decl(file, "function", &name, &sig));
            }
        }
        "class_declaration" => {
            if let Some(name) = child_name(node, source) {
                out.push(decl(file, "class", &name, &format!("class {name}")));
            }
        }
        "interface_declaration" => {
            if let Some(name) = child_name(node, source) {
                out.push(decl(file, "interface", &name, &format!("interface {name}")));
            }
        }
        "type_alias_declaration" => {
            if let Some(name) = child_name(node, source) {
                let sig = first_line(&node_text(node, source));
                out.push(decl(file, "type", &name, &sig));
            }
        }
        _ => {}
    }
}

fn collect_python(node: tree_sitter::Node, source: &str, file: &str, out: &mut Vec<serde_json::Value>) {
    match node.kind() {
        "function_definition" => {
            if let Some(name) = child_name(node, source) {
                let sig = first_line(&node_text(node, source));
                out.push(decl(file, "function", &name, &sig));
            }
        }
        "class_definition" => {
            if let Some(name) = child_name(node, source) {
                let sig = first_line(&node_text(node, source));
                out.push(decl(file, "class", &name, &sig));
            }
        }
        _ => {}
    }
}

// ─── Render (mermaid) ────────────────────────────────────────────────

fn handle_render(op: &str, params: &serde_json::Value) -> std::io::Result<serde_json::Value> {
    match op {
        "mermaid" => render_mermaid(params),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unknown render operation: {}", op),
        )),
    }
}

fn render_mermaid(params: &serde_json::Value) -> std::io::Result<serde_json::Value> {
    let source = params
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let svg = mermaid_rs_renderer::render(source).map_err(std::io::Error::other)?;
    Ok(json!({"svg": svg}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zacor_package::protocol::CapabilityReq;

    fn make_req(domain: &str, op: &str, params: serde_json::Value) -> CapabilityReq {
        CapabilityReq {
            id: 1,
            domain: domain.into(),
            op: op.into(),
            params,
        }
    }

    #[test]
    fn fs_read_string() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "hello").unwrap();

        let req = make_req("fs", "read_string", json!({"path": file.to_str().unwrap()}));
        let res = handle(&req);
        match res.result {
            CapabilityResult::Ok { data } => {
                assert_eq!(data["content"], "hello");
            }
            CapabilityResult::Error { error } => panic!("unexpected error: {}", error.message),
        }
    }

    #[test]
    fn fs_exists() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");

        let req = make_req("fs", "exists", json!({"path": file.to_str().unwrap()}));
        let res = handle(&req);
        match res.result {
            CapabilityResult::Ok { data } => assert_eq!(data["exists"], false),
            _ => panic!("expected ok"),
        }

        std::fs::write(&file, "data").unwrap();
        let res = handle(&req);
        match res.result {
            CapabilityResult::Ok { data } => assert_eq!(data["exists"], true),
            _ => panic!("expected ok"),
        }
    }

    #[test]
    fn fs_read_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "").unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();

        let req = make_req(
            "fs",
            "read_dir",
            json!({"path": dir.path().to_str().unwrap()}),
        );
        let res = handle(&req);
        match res.result {
            CapabilityResult::Ok { data } => {
                let entries = data["entries"].as_array().unwrap();
                assert_eq!(entries.len(), 2);
            }
            _ => panic!("expected ok"),
        }
    }

    #[test]
    fn fs_not_found() {
        let req = make_req(
            "fs",
            "read_string",
            json!({"path": "/nonexistent/path/file.txt"}),
        );
        let res = handle(&req);
        match res.result {
            CapabilityResult::Error { error } => {
                assert_eq!(error.kind, "not_found");
            }
            _ => panic!("expected error"),
        }
    }

    #[test]
    fn fs_write_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("output.txt");

        let encoded = protocol::base64_encode(b"written data");
        let req = make_req(
            "fs",
            "write",
            json!({"path": file.to_str().unwrap(), "content": encoded}),
        );
        let res = handle(&req);
        assert!(matches!(res.result, CapabilityResult::Ok { .. }));

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "written data");
    }

    #[test]
    fn unknown_domain_returns_error() {
        let req = make_req("unknown", "op", json!({}));
        let res = handle(&req);
        assert!(matches!(res.result, CapabilityResult::Error { .. }));
    }

    #[test]
    fn unknown_fs_op_returns_error() {
        let req = make_req("fs", "unknown_op", json!({}));
        let res = handle(&req);
        assert!(matches!(res.result, CapabilityResult::Error { .. }));
    }

    #[test]
    fn response_id_matches_request() {
        let req = CapabilityReq {
            id: 42,
            domain: "fs".into(),
            op: "exists".into(),
            params: json!({"path": "."}),
        };
        let res = handle(&req);
        assert_eq!(res.id, 42);
    }
}
