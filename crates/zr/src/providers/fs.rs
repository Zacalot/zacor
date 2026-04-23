use super::resolve_path;
use serde_json::json;
use std::path::Path;
use zacor_host::capability::CapabilityProvider;
use zacor_host::protocol::{self, CapabilityError};
use zacor_package::io::fs::FileType;

pub struct FsProvider;

impl CapabilityProvider for FsProvider {
    fn domain(&self) -> &str {
        "fs"
    }

    fn handle(
        &self,
        op: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, CapabilityError> {
        handle_fs(op, params).map_err(|error| CapabilityError::from_io(&error))
    }
}

fn handle_fs(op: &str, params: &serde_json::Value) -> std::io::Result<serde_json::Value> {
    let path_str = params.get("path").and_then(|value| value.as_str()).unwrap_or("");
    let resolved = resolve_path(path_str);

    match op {
        "read_string" => {
            let content = std::fs::read_to_string(&resolved)?;
            Ok(json!({"content": content}))
        }
        "read" => {
            let content = std::fs::read(&resolved)?;
            Ok(json!({"content": protocol::base64_encode(&content)}))
        }
        "read_dir" => {
            let mut entries = Vec::new();
            for entry in std::fs::read_dir(&resolved)? {
                let entry = entry?;
                let meta = entry.metadata()?;
                let file_type: FileType = meta.file_type().into();
                let mut value = json!({
                    "name": entry.file_name().to_string_lossy(),
                    "size": meta.len(),
                    "file_type": file_type,
                });
                if let Ok(modified) = meta.modified()
                    && let Ok(epoch) = modified.duration_since(std::time::UNIX_EPOCH)
                {
                    value["modified"] = json!(epoch.as_secs_f64());
                }
                entries.push(value);
            }
            Ok(json!({"entries": entries}))
        }
        "stat" => {
            let meta = std::fs::metadata(&resolved)?;
            let file_type: FileType = meta.file_type().into();
            let mut result = json!({"size": meta.len(), "file_type": file_type});
            if let Ok(modified) = meta.modified()
                && let Ok(epoch) = modified.duration_since(std::time::UNIX_EPOCH)
            {
                result["modified"] = json!(epoch.as_secs_f64());
            }
            if let Ok(created) = meta.created()
                && let Ok(epoch) = created.duration_since(std::time::UNIX_EPOCH)
            {
                result["created"] = json!(epoch.as_secs_f64());
            }
            Ok(result)
        }
        "exists" => Ok(json!({"exists": Path::new(&resolved).exists()})),
        "walk" => fs_walk(&resolved, params),
        "write" => {
            let content = params.get("content").and_then(|value| value.as_str()).unwrap_or("");
            let decoded = protocol::base64_decode(content)?;
            std::fs::write(&resolved, decoded)?;
            Ok(json!({}))
        }
        "create_dir_all" => {
            std::fs::create_dir_all(&resolved)?;
            Ok(json!({}))
        }
        "rename" => {
            let from = resolve_path(params.get("from").and_then(|value| value.as_str()).unwrap_or(""));
            let to = resolve_path(params.get("to").and_then(|value| value.as_str()).unwrap_or(""));
            std::fs::rename(from, to)?;
            Ok(json!({}))
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unknown fs operation: {op}"),
        )),
    }
}

fn fs_walk(resolved: &str, params: &serde_json::Value) -> std::io::Result<serde_json::Value> {
    let hidden = params.get("hidden").and_then(|value| value.as_bool()).unwrap_or(true);
    let gitignore = params
        .get("gitignore")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let max_depth = params
        .get("max_depth")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize);

    let root = Path::new(resolved);
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(hidden)
        .git_ignore(gitignore)
        .git_global(gitignore)
        .git_exclude(gitignore)
        .ignore(gitignore)
        .parents(gitignore);
    if let Some(depth) = max_depth {
        builder.max_depth(Some(depth));
    }

    let mut entries = Vec::new();
    for entry in builder.build().flatten() {
        let path = entry.path();
        if path == root {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        entries.push(json!({
            "path": relative.to_string_lossy().replace('\\', "/"),
            "is_dir": entry.file_type().map(|file_type| file_type.is_dir()).unwrap_or(false),
        }));
    }

    Ok(json!({"entries": entries}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn read_string() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "hello").unwrap();

        let data = FsProvider
            .handle("read_string", &json!({"path": path.to_str().unwrap()}))
            .unwrap();
        assert_eq!(data["content"], "hello");
    }

    #[test]
    fn exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");

        let data = FsProvider
            .handle("exists", &json!({"path": path.to_str().unwrap()}))
            .unwrap();
        assert_eq!(data["exists"], false);

        std::fs::write(&path, "data").unwrap();
        let data = FsProvider
            .handle("exists", &json!({"path": path.to_str().unwrap()}))
            .unwrap();
        assert_eq!(data["exists"], true);
    }

    #[test]
    fn read_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "").unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();

        let data = FsProvider
            .handle("read_dir", &json!({"path": dir.path().to_str().unwrap()}))
            .unwrap();
        assert_eq!(data["entries"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn not_found_maps_error_kind() {
        let error = FsProvider
            .handle("read_string", &json!({"path": "/nonexistent/path/file.txt"}))
            .unwrap_err();
        assert_eq!(error.kind, "not_found");
    }

    #[test]
    fn write_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output.txt");

        FsProvider
            .handle(
                "write",
                &json!({
                    "path": path.to_str().unwrap(),
                    "content": protocol::base64_encode(b"written data"),
                }),
            )
            .unwrap();

        assert_eq!(std::fs::read_to_string(path).unwrap(), "written data");
    }

    #[test]
    fn unknown_operation_returns_invalid_input() {
        let error = FsProvider.handle("unknown_op", &json!({})).unwrap_err();
        assert_eq!(error.kind, "invalid_input");
        assert_eq!(error.message, "unknown fs operation: unknown_op");
    }
}
