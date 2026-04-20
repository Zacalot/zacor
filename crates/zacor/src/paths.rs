use crate::error::*;
use crate::platform;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Resolve the ZR_HOME directory. Checks the ZR_HOME env var first,
/// then falls back to platform-appropriate defaults via the `dirs` crate.
pub fn zr_home() -> Result<PathBuf> {
    if let Ok(val) = env::var("ZR_HOME") {
        return Ok(PathBuf::from(val));
    }

    dirs::home_dir()
        .map(|d| d.join(".zr"))
        .ok_or_else(|| anyhow!("could not determine home directory — set ZR_HOME env var"))
}

/// The modules directory containing per-package receipt files.
pub fn modules_dir(home: &Path) -> PathBuf {
    home.join("modules")
}

/// Path to a package's receipt file.
pub fn receipt_path(home: &Path, name: &str) -> PathBuf {
    modules_dir(home).join(format!("{}.json", name))
}

pub fn store_dir(home: &Path) -> PathBuf {
    home.join("store")
}

pub fn cache_dir(home: &Path) -> PathBuf {
    home.join("cache")
}

pub fn registries_dir(home: &Path) -> PathBuf {
    home.join("registries")
}

pub fn repo_cache_dir(home: &Path) -> PathBuf {
    cache_dir(home).join("repos")
}

/// Convert a URL to a filesystem-safe slug for repo caching.
/// Strips protocol prefixes and replaces `/` with `--`.
pub fn repo_cache_path(home: &Path, url: &str) -> PathBuf {
    let slug = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .or_else(|| url.strip_prefix("git://"))
        .unwrap_or(url)
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .replace('/', "--");
    repo_cache_dir(home).join(slug)
}

/// Path to the global config.toml file.
pub fn config_path(home: &Path) -> PathBuf {
    home.join("config.toml")
}

/// Path to a package.yaml definition in the store.
pub fn definition_path(home: &Path, name: &str, version: &str) -> PathBuf {
    store_path(home, name, version).join("package.yaml")
}

/// Construct the store path for a package version.
pub fn store_path(home: &Path, name: &str, version: &str) -> PathBuf {
    store_dir(home).join(name).join(version)
}

/// Construct the full path to a stored binary using the binary base name from package.yaml.
pub fn store_binary_path(home: &Path, name: &str, version: &str, binary_name: &str) -> PathBuf {
    let filename = format!("{}{}", binary_name, platform::exe_suffix());
    store_path(home, name, version).join(filename)
}

/// Construct the full path to a stored wasm artifact. The name from
/// `package.yaml`'s `wasm:` field is used verbatim (no platform suffix).
pub fn store_wasm_path(home: &Path, name: &str, version: &str, wasm_name: &str) -> PathBuf {
    store_path(home, name, version).join(wasm_name)
}

/// Discover the project root by walking up from `start` looking for a `.zr/` directory.
/// Returns the project root (parent of `.zr/`) if found, or None.
/// Closest `.zr/` wins (first found walking upward).
/// Skips any `.zr/` whose path matches `zr_home` (the package manager's home directory).
pub fn discover_project_root(start: &Path, zr_home: &Path) -> Option<PathBuf> {
    let mut current = start;
    loop {
        let zr_dir = current.join(".zr");
        if zr_dir.is_dir() && zr_dir != zr_home {
            return Some(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

/// Compute the project data directory for a package: `<project_root>/.zr/<package_name>/`
pub fn project_data_dir(project_root: &Path, package_name: &str) -> PathBuf {
    project_root.join(".zr").join(package_name)
}

/// Compute the project local directory for a package: `<project_root>/.zr/<package_name>/.local/`
#[cfg(test)]
pub fn project_local_dir(project_root: &Path, package_name: &str) -> PathBuf {
    project_data_dir(project_root, package_name).join(".local")
}

/// Path to the project-level config: `<project_root>/.zr/config.toml`
pub fn project_config_path(project_root: &Path) -> PathBuf {
    project_root.join(".zr").join("config.toml")
}

/// Ensure all required directories exist under ZR_HOME.
pub fn ensure_dirs(home: &Path) -> Result<()> {
    fs::create_dir_all(home).context("failed to create home directory")?;
    fs::create_dir_all(modules_dir(home)).context("failed to create modules directory")?;
    fs::create_dir_all(store_dir(home)).context("failed to create store directory")?;
    fs::create_dir_all(cache_dir(home)).context("failed to create cache directory")?;
    fs::create_dir_all(registries_dir(home)).context("failed to create registries directory")?;
    fs::create_dir_all(repo_cache_dir(home)).context("failed to create repo cache directory")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zr_home_env_override() {
        let test_path = "/custom/zr/path";
        // SAFETY: test-only, single-threaded env var manipulation
        unsafe {
            env::set_var("ZR_HOME", test_path);
        }
        let home = zr_home().unwrap();
        assert_eq!(home, PathBuf::from(test_path));
        unsafe {
            env::remove_var("ZR_HOME");
        }
    }

    #[test]
    fn test_new_path_construction() {
        let home = PathBuf::from("/home/user/.zr");
        assert_eq!(modules_dir(&home), Path::new("/home/user/.zr/modules"));
        assert_eq!(
            receipt_path(&home, "my-pkg"),
            Path::new("/home/user/.zr/modules/my-pkg.json")
        );
        assert_eq!(store_dir(&home), Path::new("/home/user/.zr/store"));
        assert_eq!(cache_dir(&home), Path::new("/home/user/.zr/cache"));
        assert_eq!(config_path(&home), Path::new("/home/user/.zr/config.toml"));
    }

    #[test]
    fn test_definition_path() {
        let home = PathBuf::from("/home/user/.zr");
        let p = definition_path(&home, "my-pkg", "1.0.0");
        assert_eq!(
            p,
            Path::new("/home/user/.zr/store/my-pkg/1.0.0/package.yaml")
        );
    }

    #[test]
    fn test_store_path() {
        let home = PathBuf::from("/home/user/.zr");
        let p = store_path(&home, "uuid", "1.2.3");
        assert_eq!(p, Path::new("/home/user/.zr/store/uuid/1.2.3"));
    }

    #[test]
    fn test_store_binary_path() {
        let home = PathBuf::from("/home/user/.zr");
        let p = store_binary_path(&home, "ripgrep", "14.1.0", "rg");
        if cfg!(windows) {
            assert!(p.to_string_lossy().ends_with("rg.exe"));
        } else {
            assert!(p.to_string_lossy().ends_with("rg"));
        }
    }

    #[test]
    fn test_ensure_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("zacor_test_dirs");
        ensure_dirs(&dir).unwrap();
        assert!(modules_dir(&dir).exists());
        assert!(store_dir(&dir).exists());
        assert!(cache_dir(&dir).exists());
        assert!(registries_dir(&dir).exists());
        assert!(repo_cache_dir(&dir).exists());
    }

    #[test]
    fn test_repo_cache_path_slug() {
        let home = PathBuf::from("/home/user/.zr");
        let p = repo_cache_path(&home, "github.com/zacor-packages/p-zr-core");
        assert_eq!(
            p,
            Path::new("/home/user/.zr/cache/repos/github.com--zacor-packages--p-zr-core")
        );
    }

    #[test]
    fn test_repo_cache_path_strips_protocol() {
        let home = PathBuf::from("/home/user/.zr");
        let p = repo_cache_path(&home, "https://github.com/someone/tool.git");
        assert_eq!(
            p,
            Path::new("/home/user/.zr/cache/repos/github.com--someone--tool")
        );
    }

    // A home path that won't collide with any .zr/ in test trees
    const DUMMY_HOME: &str = if cfg!(windows) {
        "C:\\nonexistent\\.zr"
    } else {
        "/nonexistent/.zr"
    };

    #[test]
    fn test_discover_project_root_found() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("myapp");
        fs::create_dir_all(project.join(".zr")).unwrap();
        let src = project.join("src");
        fs::create_dir_all(&src).unwrap();

        let root = discover_project_root(&src, Path::new(DUMMY_HOME));
        assert_eq!(root, Some(project));
    }

    #[test]
    fn test_discover_project_root_closest_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path().join("mono");
        fs::create_dir_all(outer.join(".zr")).unwrap();
        let inner = outer.join("sub");
        fs::create_dir_all(inner.join(".zr")).unwrap();
        let deep = inner.join("src");
        fs::create_dir_all(&deep).unwrap();

        let root = discover_project_root(&deep, Path::new(DUMMY_HOME));
        assert_eq!(root, Some(inner));
    }

    #[test]
    fn test_discover_project_root_none() {
        let root = discover_project_root(
            Path::new(if cfg!(windows) { "C:\\" } else { "/" }),
            Path::new(DUMMY_HOME),
        );
        assert!(root.is_none());
    }

    #[test]
    fn test_discover_skips_zr_home() {
        // Use the real ZR_HOME so the walk doesn't find any OTHER .zr/ above
        let real_home = super::zr_home().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let start = tmp.path().join("myapp");
        fs::create_dir_all(&start).unwrap();

        // No .zr/ in the temp tree; the walk may hit ~/.zr but should skip it
        let root = discover_project_root(&start, &real_home);
        assert!(root.is_none());
    }

    #[test]
    fn test_discover_finds_project_below_zr_home() {
        let real_home = super::zr_home().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        // Real project at tmp/myapp/.zr
        let project = tmp.path().join("myapp");
        fs::create_dir_all(project.join(".zr")).unwrap();
        let src = project.join("src");
        fs::create_dir_all(&src).unwrap();

        let root = discover_project_root(&src, &real_home);
        assert_eq!(root, Some(project));
    }

    #[test]
    fn test_project_path_helpers() {
        let root = PathBuf::from("/home/user/myapp");
        assert_eq!(
            project_data_dir(&root, "bp"),
            Path::new("/home/user/myapp/.zr/bp")
        );
        assert_eq!(
            project_local_dir(&root, "bp"),
            Path::new("/home/user/myapp/.zr/bp/.local")
        );
        assert_eq!(
            project_config_path(&root),
            Path::new("/home/user/myapp/.zr/config.toml")
        );
    }
}
