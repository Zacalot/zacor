use crate::io::fs;
use std::path::{Path, PathBuf};

/// Ensure the package's project data directory exists.
///
/// Reads `ZR_DATA`, creates the directory and all parents (including `.zr/`),
/// creates the `.local/` subdirectory, ensures the project root `.gitignore`
/// contains `.zr/*/.local/`, and returns the path.
pub fn ensure_data_dir() -> Result<PathBuf, String> {
    let data_dir = std::env::var("ZR_DATA")
        .map(PathBuf::from)
        .map_err(|_| "ensure_data_dir: ZR_DATA not set".to_string())?;

    ensure_data_dir_at(&data_dir)?;
    Ok(data_dir)
}

/// Ensure a data directory exists at the given path.
///
/// Creates the directory and all parents (including `.zr/`), creates `.local/`
/// subdirectory, and ensures the project root `.gitignore` contains `.zr/*/.local/`.
///
/// Use this instead of `ensure_data_dir()` when the package needs to place
/// its data at a specific path (e.g., cwd-derived for bootstrap commands).
pub fn ensure_data_dir_at(data_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| {
        format!(
            "ensure_data_dir: failed to create {}: {e}",
            data_dir.display()
        )
    })?;

    let local_dir = data_dir.join(".local");
    fs::create_dir_all(&local_dir)
        .map_err(|e| format!("ensure_data_dir: failed to create .local/: {e}"))?;

    // data_dir is <root>/.zr/<pkg>/ — project root is grandparent
    if let Some(project_root) = data_dir.parent().and_then(|zr| zr.parent()) {
        ensure_gitignore(project_root)?;
    }

    Ok(())
}

fn ensure_gitignore(project_root: &Path) -> Result<(), String> {
    let gitignore_path = project_root.join(".gitignore");
    let pattern = ".zr/*/.local/";

    let exists = fs::exists(&gitignore_path)
        .map_err(|e| format!("ensure_data_dir: failed to stat .gitignore: {e}"))?;
    if exists {
        let contents = fs::read_string(&gitignore_path)
            .map_err(|e| format!("ensure_data_dir: failed to read .gitignore: {e}"))?;
        if contents.lines().any(|line| line.trim() == pattern) {
            return Ok(());
        }
        let sep = if contents.ends_with('\n') { "" } else { "\n" };
        let updated = format!("{contents}{sep}{pattern}\n");
        fs::write(&gitignore_path, updated.as_bytes())
            .map_err(|e| format!("ensure_data_dir: failed to write .gitignore: {e}"))?;
    } else {
        fs::write(&gitignore_path, format!("{}\n", pattern).as_bytes())
            .map_err(|e| format!("ensure_data_dir: failed to create .gitignore: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use tempfile::TempDir;

    #[test]
    fn creates_full_directory_tree() {
        let tmp = TempDir::new().unwrap();
        let data_path = tmp.path().join(".zr").join("test-pkg");

        ensure_data_dir_at(&data_path).unwrap();

        assert!(data_path.is_dir());
        assert!(data_path.join(".local").is_dir());

        // .gitignore at project root, not inside .zr/
        let gitignore = tmp.path().join(".gitignore");
        assert!(gitignore.is_file());
        let contents = std_fs::read_to_string(&gitignore).unwrap();
        assert!(contents.contains(".zr/*/.local/"));
    }

    #[test]
    fn idempotent_when_exists() {
        let tmp = TempDir::new().unwrap();
        let data_path = tmp.path().join(".zr").join("test-pkg");

        ensure_data_dir_at(&data_path).unwrap();
        ensure_data_dir_at(&data_path).unwrap();

        assert!(data_path.is_dir());
        assert!(data_path.join(".local").is_dir());

        let gitignore = tmp.path().join(".gitignore");
        let contents = std_fs::read_to_string(&gitignore).unwrap();
        assert_eq!(contents.matches(".zr/*/.local/").count(), 1);
    }

    #[test]
    fn errors_without_zr_data() {
        unsafe {
            std::env::remove_var("ZR_DATA");
        }
        let result = ensure_data_dir();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ZR_DATA not set"));
    }
}
