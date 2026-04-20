//! Filesystem IO abstraction.
//!
//! Routes through local `std::fs` or protocol capability dispatch depending
//! on the execution mode.

use super::ExecMode;
use crate::protocol;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io;
use std::path::Path;
use std::time::SystemTime;

/// Metadata about a file or directory, cross-transport compatible.
#[derive(Debug, Clone)]
pub struct Metadata {
    pub size: u64,
    pub file_type: FileType,
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
}

/// Type of filesystem entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    File,
    Dir,
    Symlink,
    Other,
}

/// A directory entry with inline metadata.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub size: u64,
    pub file_type: FileType,
    pub modified: Option<SystemTime>,
}

impl From<std::fs::FileType> for FileType {
    fn from(ft: std::fs::FileType) -> Self {
        if ft.is_file() {
            FileType::File
        } else if ft.is_dir() {
            FileType::Dir
        } else if ft.is_symlink() {
            FileType::Symlink
        } else {
            FileType::Other
        }
    }
}

impl From<std::fs::Metadata> for Metadata {
    fn from(m: std::fs::Metadata) -> Self {
        Metadata {
            size: m.len(),
            file_type: m.file_type().into(),
            created: m.created().ok(),
            modified: m.modified().ok(),
            accessed: m.accessed().ok(),
        }
    }
}

fn use_protocol() -> bool {
    super::mode() == ExecMode::ProtocolRemote
}

fn path_str(path: &Path) -> String {
    protocol::normalize_path(&path.to_string_lossy())
}

/// Read a file's entire contents as bytes.
pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    if use_protocol() {
        let data = crate::runtime::capability_call(
            "fs",
            "read",
            json!({"path": path_str(path.as_ref())}),
        )?;
        if let Some(s) = data.get("content").and_then(|v| v.as_str()) {
            return protocol::base64_decode(s);
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected response format for fs.read",
        ));
    }
    std::fs::read(path)
}

/// Read a file's entire contents as a UTF-8 string.
pub fn read_string(path: impl AsRef<Path>) -> io::Result<String> {
    if use_protocol() {
        let data = crate::runtime::capability_call(
            "fs",
            "read_string",
            json!({"path": path_str(path.as_ref())}),
        )?;
        return data
            .get("content")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected response format for fs.read_string",
                )
            });
    }
    std::fs::read_to_string(path)
}

/// List directory entries with inline metadata.
pub fn read_dir(path: impl AsRef<Path>) -> io::Result<Vec<DirEntry>> {
    if use_protocol() {
        let data = crate::runtime::capability_call(
            "fs",
            "read_dir",
            json!({"path": path_str(path.as_ref())}),
        )?;
        let entries = data
            .get("entries")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected response format for fs.read_dir",
                )
            })?;
        return entries
            .iter()
            .map(|e| {
                Ok(DirEntry {
                    name: e
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    size: e.get("size").and_then(|v| v.as_u64()).unwrap_or(0),
                    file_type: e
                        .get("file_type")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or(FileType::Other),
                    modified: None,
                })
            })
            .collect();
    }
    read_dir_local(path.as_ref())
}

fn read_dir_local(path: &Path) -> io::Result<Vec<DirEntry>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        entries.push(DirEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            size: metadata.len(),
            file_type: metadata.file_type().into(),
            modified: metadata.modified().ok(),
        });
    }
    Ok(entries)
}

/// Options for `walk`. Defaults mirror `ignore::WalkBuilder` behavior:
/// skip hidden (dot-prefixed) entries and respect `.gitignore`.
#[derive(Debug, Clone)]
pub struct WalkOptions {
    /// Skip hidden (dot-prefixed) entries. Default `true`.
    pub hidden: bool,
    /// Respect `.gitignore`, `.ignore`, global/exclude files, and parents.
    /// Default `true`. When `false`, every non-hidden entry is visited.
    pub gitignore: bool,
    /// Limit walk depth from root. `None` = unlimited.
    pub max_depth: Option<usize>,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            hidden: true,
            gitignore: true,
            max_depth: None,
        }
    }
}

/// A walk result entry, path relative to the walked root in forward-slash form.
#[derive(Debug, Clone)]
pub struct WalkEntry {
    pub path: String,
    pub is_dir: bool,
}

/// Recursively walk a directory honoring `.gitignore` and hidden filters.
///
/// Under protocol dispatch (native subprocess or wasm), this routes through
/// the host's `fs.walk` capability which uses the `ignore` crate — so the
/// same rules apply regardless of substrate. Under `ExecMode::Local` (direct
/// API use without a protocol peer), falls back to a plain recursive walk
/// without gitignore support.
pub fn walk(path: impl AsRef<Path>, options: &WalkOptions) -> io::Result<Vec<WalkEntry>> {
    if super::mode() != ExecMode::Local {
        let data = crate::runtime::capability_call(
            "fs",
            "walk",
            json!({
                "path": path_str(path.as_ref()),
                "hidden": options.hidden,
                "gitignore": options.gitignore,
                "max_depth": options.max_depth,
            }),
        )?;
        let entries = data.get("entries").and_then(|v| v.as_array()).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected response format for fs.walk",
            )
        })?;
        return entries
            .iter()
            .map(|e| {
                Ok(WalkEntry {
                    path: e
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    is_dir: e.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false),
                })
            })
            .collect();
    }
    walk_local(path.as_ref(), options)
}

fn walk_local(root: &Path, options: &WalkOptions) -> io::Result<Vec<WalkEntry>> {
    let mut out = Vec::new();
    walk_local_recurse(root, root, 0, options, &mut out)?;
    Ok(out)
}

fn walk_local_recurse(
    root: &Path,
    current: &Path,
    depth: usize,
    options: &WalkOptions,
    out: &mut Vec<WalkEntry>,
) -> io::Result<()> {
    if let Some(max) = options.max_depth
        && depth >= max
    {
        return Ok(());
    }
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if options.hidden && name.starts_with('.') {
            continue;
        }
        let full = current.join(&name);
        let ft = entry.file_type()?;
        let is_dir = ft.is_dir();
        let rel = full.strip_prefix(root).map_err(io::Error::other)?;
        out.push(WalkEntry {
            path: rel.to_string_lossy().replace('\\', "/"),
            is_dir,
        });
        if is_dir {
            walk_local_recurse(root, &full, depth + 1, options, out)?;
        }
    }
    Ok(())
}

/// Get metadata for a path.
pub fn stat(path: impl AsRef<Path>) -> io::Result<Metadata> {
    if use_protocol() {
        let data = crate::runtime::capability_call(
            "fs",
            "stat",
            json!({"path": path_str(path.as_ref())}),
        )?;
        return Ok(Metadata {
            size: data.get("size").and_then(|v| v.as_u64()).unwrap_or(0),
            file_type: data
                .get("file_type")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or(FileType::Other),
            created: None,
            modified: None,
            accessed: None,
        });
    }
    std::fs::metadata(path).map(Metadata::from)
}

/// Check if a path exists.
pub fn exists(path: impl AsRef<Path>) -> io::Result<bool> {
    if use_protocol() {
        let data = crate::runtime::capability_call(
            "fs",
            "exists",
            json!({"path": path_str(path.as_ref())}),
        )?;
        return Ok(data
            .get("exists")
            .and_then(|v| v.as_bool())
            .unwrap_or(false));
    }
    Ok(path.as_ref().exists())
}

/// Return true when `path` is a directory. Safer than `Path::is_dir()` under
/// `ExecMode::ProtocolRemote` — `Path::is_dir()` calls `std::fs::metadata`
/// directly and so fails in wasm sandboxes that don't preopen the path.
pub fn is_dir(path: impl AsRef<Path>) -> bool {
    stat(path)
        .map(|m| m.file_type == FileType::Dir)
        .unwrap_or(false)
}

/// Return true when `path` is a regular file. See `is_dir` for the rationale
/// behind routing through `stat` instead of `Path::is_file()`.
pub fn is_file(path: impl AsRef<Path>) -> bool {
    stat(path)
        .map(|m| m.file_type == FileType::File)
        .unwrap_or(false)
}

/// Create a directory and all of its parents. Routes through the
/// `fs.create_dir_all` capability under `ExecMode::ProtocolRemote`.
pub fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    if use_protocol() {
        crate::runtime::capability_call(
            "fs",
            "create_dir_all",
            json!({"path": path_str(path.as_ref())}),
        )?;
        return Ok(());
    }
    std::fs::create_dir_all(path)
}

/// Rename (move) a file or directory. Routes through the `fs.rename`
/// capability under `ExecMode::ProtocolRemote`.
pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    if use_protocol() {
        crate::runtime::capability_call(
            "fs",
            "rename",
            json!({
                "from": path_str(from.as_ref()),
                "to": path_str(to.as_ref()),
            }),
        )?;
        return Ok(());
    }
    std::fs::rename(from, to)
}

/// Write data to a file, creating it if it doesn't exist.
pub fn write(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> io::Result<()> {
    if use_protocol() {
        let encoded = protocol::base64_encode(data.as_ref());
        crate::runtime::capability_call(
            "fs",
            "write",
            json!({"path": path_str(path.as_ref()), "content": encoded}),
        )?;
        return Ok(());
    }
    std::fs::write(path, data)
}

/// Open a file for streaming reads. Returns a boxed reader.
pub fn open_stream(path: impl AsRef<Path>) -> io::Result<Box<dyn io::Read>> {
    if use_protocol() {
        // In protocol mode, read entire file via capability and wrap in Cursor
        let data = crate::runtime::capability_call(
            "fs",
            "read",
            json!({"path": path_str(path.as_ref())}),
        )?;
        if let Some(s) = data.get("content").and_then(|v| v.as_str()) {
            let bytes = protocol::base64_decode(s)?;
            return Ok(Box::new(io::Cursor::new(bytes)));
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected response format for fs.open_stream",
        ));
    }
    let file = std::fs::File::open(path)?;
    Ok(Box::new(io::BufReader::new(file)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn test_read_string_local() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "hello world").unwrap();
        let content = read_string(&file).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_read_local() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.bin");
        std::fs::write(&file, b"\x00\x01\x02").unwrap();
        let content = read(&file).unwrap();
        assert_eq!(content, b"\x00\x01\x02");
    }

    #[test]
    fn test_read_dir_local() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "aaa").unwrap();
        std::fs::write(dir.path().join("b.txt"), "bb").unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();

        let entries = read_dir(dir.path()).unwrap();
        assert_eq!(entries.len(), 3);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"b.txt"));
        assert!(names.contains(&"subdir"));

        let subdir = entries.iter().find(|e| e.name == "subdir").unwrap();
        assert_eq!(subdir.file_type, FileType::Dir);
    }

    #[test]
    fn test_stat_local() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "hello").unwrap();
        let meta = stat(&file).unwrap();
        assert_eq!(meta.size, 5);
        assert_eq!(meta.file_type, FileType::File);
    }

    #[test]
    fn test_exists_local() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        assert!(!exists(&file).unwrap());
        std::fs::write(&file, "hello").unwrap();
        assert!(exists(&file).unwrap());
    }

    #[test]
    fn test_write_local() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("output.txt");
        write(&file, "written data").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "written data");
    }

    #[test]
    fn test_open_stream_local() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("stream.txt");
        std::fs::write(&file, "streaming content").unwrap();
        let mut reader = open_stream(&file).unwrap();
        let mut buf = String::new();
        reader.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "streaming content");
    }

    #[test]
    fn test_read_string_not_found() {
        let result = read_string("/nonexistent/path/file.txt");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    }
}
