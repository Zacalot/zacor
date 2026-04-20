use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use zacor_package::io::fs::{self, FileType, WalkOptions};

zacor_package::include_args!();

#[derive(Serialize)]
pub struct TreeRecord {
    pub line: String,
}

pub fn tree(path: &Path, depth: Option<usize>) -> Result<Vec<TreeRecord>, String> {
    let meta = fs::stat(path)
        .map_err(|e| format!("tree: stat '{}': {}", path.display(), e))?;
    if meta.file_type != FileType::Dir {
        return Err(format!("tree: '{}' is not a directory", path.display()));
    }

    let entries = fs::walk(
        path,
        &WalkOptions {
            max_depth: depth,
            ..WalkOptions::default()
        },
    )
    .map_err(|e| format!("tree: walk '{}': {}", path.display(), e))?;

    let mut root = TreeNode::default();
    for entry in entries {
        let parts: Vec<&str> = entry.path.split('/').collect();
        root.insert(&parts, entry.is_dir);
    }

    let root_name = path
        .file_name()
        .map(|n| format!("{}/", n.to_string_lossy()))
        .unwrap_or_else(|| ".".to_string());

    let mut records = vec![TreeRecord { line: root_name }];
    render(&root, "", &mut records, 1, depth);
    Ok(records)
}

#[derive(Default)]
struct TreeNode {
    children: BTreeMap<String, TreeNode>,
    is_dir: bool,
}

impl TreeNode {
    fn insert(&mut self, parts: &[&str], is_dir: bool) {
        if parts.is_empty() {
            return;
        }
        let child = self.children.entry(parts[0].to_string()).or_default();
        if parts.len() == 1 {
            child.is_dir = is_dir;
        } else {
            child.is_dir = true;
            child.insert(&parts[1..], is_dir);
        }
    }
}

fn render(
    node: &TreeNode,
    prefix: &str,
    records: &mut Vec<TreeRecord>,
    current_depth: usize,
    max_depth: Option<usize>,
) {
    if let Some(max) = max_depth {
        if current_depth > max {
            return;
        }
    }
    let entries: Vec<_> = node.children.iter().collect();
    for (i, (name, child)) in entries.iter().enumerate() {
        let is_last = i == entries.len() - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let display = if child.is_dir {
            format!("{name}/")
        } else {
            name.to_string()
        };
        records.push(TreeRecord {
            line: format!("{prefix}{connector}{display}"),
        });
        let new_prefix = if is_last {
            format!("{prefix}    ")
        } else {
            format!("{prefix}│   ")
        };
        render(child, &new_prefix, records, current_depth + 1, max_depth);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> TempDir {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("src/cmd")).unwrap();
        fs::write(tmp.path().join("src/lib.rs"), "fn main() {}").unwrap();
        fs::write(tmp.path().join("src/cmd/foo.rs"), "fn foo() {}").unwrap();
        fs::write(tmp.path().join("src/cmd/bar.rs"), "fn bar() {}").unwrap();
        fs::write(tmp.path().join("README.md"), "# Hello").unwrap();
        tmp
    }

    #[test]
    fn tree_from_root() {
        let tmp = setup();
        let result = tree(tmp.path(), None).unwrap();
        assert!(!result.is_empty());
        let lines: Vec<&str> = result.iter().map(|r| r.line.as_str()).collect();
        assert!(lines.iter().any(|l| l.contains("src/")));
        assert!(lines.iter().any(|l| l.contains("README.md")));
    }

    #[test]
    fn tree_scoped_to_dir() {
        let tmp = setup();
        let result = tree(&tmp.path().join("src"), None).unwrap();
        let lines: Vec<&str> = result.iter().map(|r| r.line.as_str()).collect();
        assert_eq!(lines[0], "src/");
        assert!(lines.iter().any(|l| l.contains("lib.rs")));
        assert!(lines.iter().any(|l| l.contains("cmd/")));
    }

    #[test]
    fn tree_depth_limit() {
        let tmp = setup();
        let result = tree(tmp.path(), Some(1)).unwrap();
        let lines: Vec<&str> = result.iter().map(|r| r.line.as_str()).collect();
        assert!(lines.iter().any(|l| l.contains("src/")));
        assert!(!lines.iter().any(|l| l.contains("foo.rs")));
    }

    #[test]
    fn tree_nonexistent_dir() {
        let tmp = setup();
        let result = tree(&tmp.path().join("nonexistent"), None);
        assert!(result.is_err());
    }

    #[test]
    fn tree_skips_hidden_entries() {
        let tmp = setup();
        fs::write(tmp.path().join(".hidden.txt"), "secret").unwrap();
        let result = tree(tmp.path(), None).unwrap();
        let lines: Vec<&str> = result.iter().map(|r| r.line.as_str()).collect();
        assert!(!lines.iter().any(|l| l.contains(".hidden")));
    }

    // Gitignore support lives in the host's `fs.walk` capability. The unit
    // test runs under `ExecMode::Local` where the SDK's local fallback walker
    // doesn't consult `.gitignore` — end-to-end gitignore filtering is
    // verified via manual `zr tree` dispatch in a real project tree.

    #[test]
    fn tree_box_drawing_format() {
        let tmp = setup();
        let result = tree(tmp.path(), None).unwrap();
        let lines: Vec<&str> = result.iter().map(|r| r.line.as_str()).collect();
        // Should use box-drawing characters
        assert!(lines.iter().any(|l| l.contains("├── ") || l.contains("└── ")));
    }
}
