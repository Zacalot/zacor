use crate::config::{GlobalConfig, RegistryConfig};
use crate::error::*;
use crate::paths;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, SystemTime};

const DEFAULT_REGISTRY_NAME: &str = "default";
const DEFAULT_REGISTRY_URL: &str = "https://github.com/zacor-packages/registry";
const STALENESS_THRESHOLD: Duration = Duration::from_secs(24 * 60 * 60); // 24 hours
const SUPPORTED_SCHEMA: u64 = 1;

/// A resolved registry entry for a specific package version.
#[derive(Debug, Clone)]
pub struct RegistryEntry {
    pub name: String,
    pub version: String,
    pub release: Option<String>,
    pub repo: Option<String>,
    pub path: Option<String>,
    pub tag: Option<String>,
    pub registry_name: String,
}

/// Parsed registry index.toml for a package.
#[derive(Debug, serde::Deserialize)]
struct PackageIndex {
    schema: u64,
    #[serde(default)]
    versions: Vec<VersionEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct VersionEntry {
    version: String,
    #[serde(default)]
    release: Option<String>,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    yanked: Option<bool>,
    #[serde(default)]
    tag: Option<String>,
}

/// Resolve a package from configured registries.
/// Searches registries in config order. If `registry_name` is provided, only that registry is searched.
/// If `version` is None, returns the latest non-yanked version.
pub fn resolve(
    home: &Path,
    name: &str,
    version: Option<&str>,
    registry_name: Option<&str>,
    config: &GlobalConfig,
) -> Result<RegistryEntry> {
    let registries = &config.registries;
    if registries.is_empty() {
        bail!("no registries configured\nhint: add a registry with 'zacor registry add <url>'");
    }

    let search_registries: Vec<&RegistryConfig> = if let Some(rn) = registry_name {
        registries
            .iter()
            .filter(|r| r.name == rn)
            .collect::<Vec<_>>()
    } else {
        registries.iter().collect()
    };

    if search_registries.is_empty() {
        if let Some(rn) = registry_name {
            bail!("registry '{}' not found in config", rn);
        }
    }

    for reg in &search_registries {
        let reg_dir = paths::registries_dir(home).join(&reg.name);

        // Sync if needed
        sync_registry_if_needed(home, reg)?;

        // Try to find the package
        let index_path = reg_dir.join("packages").join(name).join("index.toml");
        if !index_path.exists() {
            continue;
        }

        let contents = fs::read_to_string(&index_path)
            .with_context(|| format!("failed to read index for package '{}'", name))?;
        let index: PackageIndex = toml::from_str(&contents)
            .with_context(|| format!("failed to parse index for package '{}'", name))?;

        if index.schema > SUPPORTED_SCHEMA {
            bail!(
                "package '{}' requires a newer version of zacor (schema {})\nhint: upgrade zacor to install this package",
                name,
                index.schema
            );
        }

        let entry = if let Some(ver) = version {
            // Find exact version
            let found = index.versions.iter().find(|v| v.version == ver);
            match found {
                Some(v) => {
                    if v.yanked.unwrap_or(false) {
                        eprintln!("warning: version {} of '{}' has been yanked", ver, name);
                    }
                    v
                }
                None => {
                    let available: Vec<&str> =
                        index.versions.iter().map(|v| v.version.as_str()).collect();
                    bail!(
                        "version '{}' not found for '{}'\navailable versions: {}",
                        ver,
                        name,
                        available.join(", ")
                    );
                }
            }
        } else {
            // Find latest non-yanked version
            let latest = index
                .versions
                .iter()
                .filter(|v| !v.yanked.unwrap_or(false))
                .max_by(|a, b| {
                    let va = semver::Version::parse(&a.version).ok();
                    let vb = semver::Version::parse(&b.version).ok();
                    match (va, vb) {
                        (Some(a), Some(b)) => a.cmp(&b),
                        (Some(_), None) => std::cmp::Ordering::Greater,
                        (None, Some(_)) => std::cmp::Ordering::Less,
                        (None, None) => a.version.cmp(&b.version),
                    }
                });
            match latest {
                Some(v) => v,
                None => bail!("no non-yanked versions found for '{}'", name),
            }
        };

        return Ok(RegistryEntry {
            name: name.to_string(),
            version: entry.version.clone(),
            release: entry.release.clone(),
            repo: entry.repo.clone(),
            path: entry.path.clone(),
            tag: entry.tag.clone(),
            registry_name: reg.name.clone(),
        });
    }

    bail!(
        "package '{}' not found in any configured registry\nhint: check the package name or add a registry with 'zacor registry add <url>'",
        name
    );
}

/// Seed the default registry into config if no registries are configured.
/// Returns true if the config was modified.
pub fn seed_default_if_empty(home: &Path, config: &mut GlobalConfig) -> Result<bool> {
    if !config.registries.is_empty() {
        return Ok(false);
    }

    config.registries.push(RegistryConfig {
        name: DEFAULT_REGISTRY_NAME.to_string(),
        url: DEFAULT_REGISTRY_URL.to_string(),
    });

    crate::config::write_global(home, config)?;
    eprintln!("added default registry");
    Ok(true)
}

/// Sync a registry if it's stale or has never been synced.
fn sync_registry_if_needed(home: &Path, reg: &RegistryConfig) -> Result<()> {
    let reg_dir = paths::registries_dir(home).join(&reg.name);

    if !reg_dir.exists() {
        return sync_registry(home, reg);
    }

    // Check staleness
    if is_stale(&reg_dir) {
        if let Err(e) = sync_registry(home, reg) {
            eprintln!("warning: failed to sync registry '{}': {}", reg.name, e);
            eprintln!("using cached copy");
        }
    }

    Ok(())
}

/// Sync a registry by cloning or fetching.
pub fn sync_registry(home: &Path, reg: &RegistryConfig) -> Result<()> {
    let reg_dir = paths::registries_dir(home).join(&reg.name);

    if reg_dir.exists() {
        eprintln!("syncing registry '{}'...", reg.name);
        let output = Command::new("git")
            .args(["fetch", "origin"])
            .current_dir(&reg_dir)
            .output()
            .context("failed to run git fetch for registry")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "git fetch failed for registry '{}': {}",
                reg.name,
                stderr.trim()
            );
        }
        let output = Command::new("git")
            .args(["reset", "--hard", "origin/HEAD"])
            .current_dir(&reg_dir)
            .output()
            .context("failed to run git reset for registry")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "git reset failed for registry '{}': {}",
                reg.name,
                stderr.trim()
            );
        }
    } else {
        eprintln!("cloning registry '{}'...", reg.name);
        let output = Command::new("git")
            .args(["clone", "--depth", "1", &reg.url])
            .arg(&reg_dir)
            .output()
            .context("failed to clone registry")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("failed to clone registry '{}': {}", reg.name, stderr.trim());
        }
    }

    // Touch a marker file to track sync time
    let marker = reg_dir.join(".zr-last-sync");
    let _ = fs::write(&marker, "");

    Ok(())
}

/// Check if a registry cache is stale (last sync older than threshold).
fn is_stale(reg_dir: &Path) -> bool {
    let marker = reg_dir.join(".zr-last-sync");
    match fs::metadata(&marker).and_then(|m| m.modified()) {
        Ok(modified) => {
            SystemTime::now()
                .duration_since(modified)
                .unwrap_or(STALENESS_THRESHOLD)
                >= STALENESS_THRESHOLD
        }
        Err(_) => true, // No marker = treat as stale
    }
}

/// Get the last sync time for a registry (for display).
pub fn last_sync_time(home: &Path, registry_name: &str) -> Option<SystemTime> {
    let marker = paths::registries_dir(home)
        .join(registry_name)
        .join(".zr-last-sync");
    fs::metadata(&marker).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util;

    fn make_registry_index(dir: &Path, pkg_name: &str, index_toml: &str) {
        let pkg_dir = dir.join("packages").join(pkg_name);
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("index.toml"), index_toml).unwrap();
    }

    #[test]
    fn test_resolve_latest_version() {
        let home = test_util::temp_home("registry");
        let reg_dir = paths::registries_dir(home.path()).join("test-reg");
        fs::create_dir_all(&reg_dir).unwrap();
        // Touch sync marker so it's not stale
        fs::write(reg_dir.join(".zr-last-sync"), "").unwrap();

        make_registry_index(
            &reg_dir,
            "echo",
            r#"
schema = 1
description = "Display text"

[[versions]]
version = "0.1.0"
release = "zacor-packages/zr-echo"

[[versions]]
version = "0.2.0"
release = "zacor-packages/zr-echo"
repo = "github.com/zacor-packages/p-zr-core"
path = "echo"
"#,
        );

        let mut config = GlobalConfig::default();
        config.registries.push(RegistryConfig {
            name: "test-reg".to_string(),
            url: "https://example.com/test".to_string(),
        });

        let entry = resolve(home.path(), "echo", None, None, &config).unwrap();
        assert_eq!(entry.name, "echo");
        assert_eq!(entry.version, "0.2.0");
        assert_eq!(entry.release, Some("zacor-packages/zr-echo".to_string()));
        assert_eq!(entry.path, Some("echo".to_string()));
    }

    #[test]
    fn test_resolve_specific_version() {
        let home = test_util::temp_home("registry");
        let reg_dir = paths::registries_dir(home.path()).join("test-reg");
        fs::create_dir_all(&reg_dir).unwrap();
        fs::write(reg_dir.join(".zr-last-sync"), "").unwrap();

        make_registry_index(
            &reg_dir,
            "echo",
            r#"
schema = 1

[[versions]]
version = "0.1.0"
release = "zacor-packages/zr-echo"

[[versions]]
version = "0.2.0"
release = "zacor-packages/zr-echo"
"#,
        );

        let mut config = GlobalConfig::default();
        config.registries.push(RegistryConfig {
            name: "test-reg".to_string(),
            url: "https://example.com/test".to_string(),
        });

        let entry = resolve(home.path(), "echo", Some("0.1.0"), None, &config).unwrap();
        assert_eq!(entry.version, "0.1.0");
    }

    #[test]
    fn test_resolve_skips_yanked() {
        let home = test_util::temp_home("registry");
        let reg_dir = paths::registries_dir(home.path()).join("test-reg");
        fs::create_dir_all(&reg_dir).unwrap();
        fs::write(reg_dir.join(".zr-last-sync"), "").unwrap();

        make_registry_index(
            &reg_dir,
            "echo",
            r#"
schema = 1

[[versions]]
version = "0.1.0"
release = "zacor-packages/zr-echo"

[[versions]]
version = "0.2.0"
release = "zacor-packages/zr-echo"
yanked = true
"#,
        );

        let mut config = GlobalConfig::default();
        config.registries.push(RegistryConfig {
            name: "test-reg".to_string(),
            url: "https://example.com/test".to_string(),
        });

        let entry = resolve(home.path(), "echo", None, None, &config).unwrap();
        assert_eq!(entry.version, "0.1.0");
    }

    #[test]
    fn test_resolve_version_not_found() {
        let home = test_util::temp_home("registry");
        let reg_dir = paths::registries_dir(home.path()).join("test-reg");
        fs::create_dir_all(&reg_dir).unwrap();
        fs::write(reg_dir.join(".zr-last-sync"), "").unwrap();

        make_registry_index(
            &reg_dir,
            "echo",
            r#"
schema = 1
[[versions]]
version = "0.1.0"
release = "zacor-packages/zr-echo"
"#,
        );

        let mut config = GlobalConfig::default();
        config.registries.push(RegistryConfig {
            name: "test-reg".to_string(),
            url: "https://example.com/test".to_string(),
        });

        let err = resolve(home.path(), "echo", Some("9.9.9"), None, &config).unwrap_err();
        assert!(err.to_string().contains("9.9.9"), "got: {}", err);
    }

    #[test]
    fn test_resolve_package_not_found() {
        let home = test_util::temp_home("registry");
        let reg_dir = paths::registries_dir(home.path()).join("test-reg");
        fs::create_dir_all(&reg_dir).unwrap();
        fs::write(reg_dir.join(".zr-last-sync"), "").unwrap();

        let mut config = GlobalConfig::default();
        config.registries.push(RegistryConfig {
            name: "test-reg".to_string(),
            url: "https://example.com/test".to_string(),
        });

        let err = resolve(home.path(), "nonexistent", None, None, &config).unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {}", err);
    }

    #[test]
    fn test_resolve_schema_too_new() {
        let home = test_util::temp_home("registry");
        let reg_dir = paths::registries_dir(home.path()).join("test-reg");
        fs::create_dir_all(&reg_dir).unwrap();
        fs::write(reg_dir.join(".zr-last-sync"), "").unwrap();

        make_registry_index(
            &reg_dir,
            "future-pkg",
            r#"
schema = 2
[[versions]]
version = "1.0.0"
release = "someone/future-pkg"
"#,
        );

        let mut config = GlobalConfig::default();
        config.registries.push(RegistryConfig {
            name: "test-reg".to_string(),
            url: "https://example.com/test".to_string(),
        });

        let err = resolve(home.path(), "future-pkg", None, None, &config).unwrap_err();
        assert!(err.to_string().contains("newer version"), "got: {}", err);
    }

    #[test]
    fn test_no_registries_configured() {
        let home = test_util::temp_home("registry");
        let config = GlobalConfig::default();
        let err = resolve(home.path(), "echo", None, None, &config).unwrap_err();
        assert!(err.to_string().contains("no registries"), "got: {}", err);
    }

    #[test]
    fn test_seed_default() {
        let home = test_util::temp_home("registry");
        let mut config = GlobalConfig::default();

        let seeded = seed_default_if_empty(home.path(), &mut config).unwrap();
        assert!(seeded);
        assert_eq!(config.registries.len(), 1);
        assert_eq!(config.registries[0].name, "default");

        // Second call should not seed again
        let seeded = seed_default_if_empty(home.path(), &mut config).unwrap();
        assert!(!seeded);
        assert_eq!(config.registries.len(), 1);
    }

    #[test]
    fn test_staleness_check() {
        let tmp = tempfile::tempdir().unwrap();
        // Fresh marker
        fs::write(tmp.path().join(".zr-last-sync"), "").unwrap();
        assert!(!is_stale(tmp.path()));
    }

    #[test]
    fn test_staleness_no_marker() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(is_stale(tmp.path()));
    }
}
