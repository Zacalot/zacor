use crate::error::*;
use crate::package_definition;
use crate::paths;
use crate::platform;
use std::fs;
use std::path::Path;

/// Place a package into the store at `store/{name}/{version}/`.
/// `content_dir` must contain either a sidecar `package.yaml` or a
/// `.wasm` artifact with an embedded manifest custom section.
/// All files in the content directory are copied to the store.
/// Files matching the package's declared binary get executable permissions on Unix.
pub fn put(home: &Path, name: &str, version: &str, content_dir: &Path) -> Result<()> {
    let yaml_path = content_dir.join("package.yaml");
    let wasm_path = crate::wasm_manifest::find_wasm_in_store(content_dir)?;
    if !yaml_path.exists() && wasm_path.is_none() {
        bail!(
            "content directory contains neither package.yaml nor a .wasm artifact with an embedded manifest"
        );
    }

    let dir = paths::store_path(home, name, version);
    if dir.exists() {
        fs::remove_dir_all(&dir).context("failed to replace existing store directory")?;
    }
    fs::create_dir_all(&dir).context("failed to create store directory")?;

    copy_dir_contents(content_dir, &dir)?;

    // Set executable bits on declared binary (native-only concern).
    // Prefer the sidecar yaml for this check when present; wasm packages
    // don't need chmod.
    if yaml_path.exists()
        && let Ok(def) = package_definition::parse_file(&yaml_path)
        && let Some(ref binary_name) = def.binary
    {
        let bin_filename = format!("{}{}", binary_name, platform::exe_suffix());
        let bin_dest = dir.join(&bin_filename);
        if bin_dest.exists() {
            set_executable(&bin_dest)?;
        }
    }

    Ok(())
}

fn copy_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    for entry in fs::read_dir(src).context("failed to read content directory")? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if file_type.is_dir() {
            fs::create_dir_all(&dest)?;
            copy_dir_contents(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), &dest).context("failed to copy file to store")?;
        }
    }
    Ok(())
}

/// Remove a single version from the store.
pub fn remove_version(home: &Path, name: &str, version: &str) -> Result<()> {
    let dir = paths::store_path(home, name, version);
    if dir.exists() {
        fs::remove_dir_all(&dir).context("failed to remove version directory")?;
    }
    Ok(())
}

/// Remove an entire package from the store (all versions).
pub fn remove(home: &Path, name: &str) -> Result<()> {
    let store = paths::store_dir(home).join(name);
    if store.exists() {
        fs::remove_dir_all(&store).context("failed to remove store directory")?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o111);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(windows)]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util;

    fn create_content_dir(yaml: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.yaml"), yaml).unwrap();
        dir
    }

    #[test]
    fn test_put_binary_package() {
        let home = test_util::temp_home("store");
        let content = create_content_dir(
            "name: testmod\nversion: \"1.0.0\"\nbinary: testmod\ncommands:\n  default:\n    description: test\n",
        );
        test_util::create_dummy_binary(
            content.path(),
            &format!("testmod{}", platform::exe_suffix()),
        );

        put(home.path(), "testmod", "1.0.0", content.path()).unwrap();
        assert!(paths::definition_path(home.path(), "testmod", "1.0.0").exists());
        assert!(paths::store_binary_path(home.path(), "testmod", "1.0.0", "testmod").exists());
    }

    #[test]
    fn test_put_definition_only() {
        let home = test_util::temp_home("store");
        let content = create_content_dir(
            "name: my-wrapper\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: test\n",
        );

        put(home.path(), "my-wrapper", "1.0.0", content.path()).unwrap();
        assert!(paths::definition_path(home.path(), "my-wrapper", "1.0.0").exists());
    }

    #[test]
    fn test_put_missing_package_yaml() {
        let home = test_util::temp_home("store");
        let content = tempfile::tempdir().unwrap();

        let result = put(home.path(), "test", "1.0.0", content.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("package.yaml"));
    }

    #[test]
    fn test_put_multi_file_package() {
        let home = test_util::temp_home("store");
        let content = create_content_dir(
            "name: py-tool\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: test\n",
        );
        fs::write(content.path().join("main.py"), "print('hello')").unwrap();
        fs::create_dir_all(content.path().join("lib")).unwrap();
        fs::write(content.path().join("lib").join("utils.py"), "# utils").unwrap();

        put(home.path(), "py-tool", "1.0.0", content.path()).unwrap();
        let store = paths::store_path(home.path(), "py-tool", "1.0.0");
        assert!(store.join("package.yaml").exists());
        assert!(store.join("main.py").exists());
        assert!(store.join("lib").join("utils.py").exists());
    }

    #[test]
    fn test_put_replaces_existing_same_version_directory() {
        let home = test_util::temp_home("store");
        let initial = create_content_dir(
            "name: testmod\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: test\n",
        );
        fs::write(initial.path().join("stale.txt"), "old").unwrap();
        put(home.path(), "testmod", "1.0.0", initial.path()).unwrap();

        let replacement = create_content_dir(
            "name: testmod\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: updated\n",
        );
        put(home.path(), "testmod", "1.0.0", replacement.path()).unwrap();

        let store = paths::store_path(home.path(), "testmod", "1.0.0");
        assert!(!store.join("stale.txt").exists());
        let yaml = fs::read_to_string(store.join("package.yaml")).unwrap();
        assert!(yaml.contains("updated"), "got: {}", yaml);
    }

    #[test]
    fn test_remove_version() {
        let home = test_util::temp_home("store");
        let content = create_content_dir(
            "name: testmod\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: test\n",
        );

        put(home.path(), "testmod", "1.0.0", content.path()).unwrap();
        put(home.path(), "testmod", "2.0.0", content.path()).unwrap();
        assert!(paths::store_path(home.path(), "testmod", "1.0.0").exists());
        assert!(paths::store_path(home.path(), "testmod", "2.0.0").exists());

        remove_version(home.path(), "testmod", "1.0.0").unwrap();
        assert!(!paths::store_path(home.path(), "testmod", "1.0.0").exists());
        assert!(paths::store_path(home.path(), "testmod", "2.0.0").exists());
    }

    #[test]
    fn test_remove_all() {
        let home = test_util::temp_home("store");
        let content = create_content_dir(
            "name: testmod\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: test\n",
        );

        put(home.path(), "testmod", "1.0.0", content.path()).unwrap();
        put(home.path(), "testmod", "2.0.0", content.path()).unwrap();

        remove(home.path(), "testmod").unwrap();
        assert!(!paths::store_dir(home.path()).join("testmod").exists());
    }
}
