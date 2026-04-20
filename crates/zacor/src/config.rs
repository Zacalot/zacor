use crate::error::*;
use crate::paths;
use crate::receipt::Receipt;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::Path;

/// A configured registry source.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegistryConfig {
    pub name: String,
    pub url: String,
}

/// Global config file structure (config.toml).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GlobalConfig {
    #[serde(default)]
    pub zr: BTreeMap<String, String>,
    #[serde(default)]
    pub packages: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default)]
    pub registries: Vec<RegistryConfig>,
}

/// Read config.toml. Returns default if file doesn't exist.
pub fn read_global(home: &Path) -> Result<GlobalConfig> {
    let path = paths::config_path(home);
    if !path.exists() {
        return Ok(GlobalConfig::default());
    }
    let contents = fs::read_to_string(&path).context("failed to read config.toml")?;
    parse_global_config(&contents)
}

/// Read project-level config.toml from `.zr/config.toml`.
/// Returns default if the file doesn't exist.
pub fn read_project(project_root: &Path) -> Result<GlobalConfig> {
    let path = paths::project_config_path(project_root);
    if !path.exists() {
        return Ok(GlobalConfig::default());
    }
    let contents = fs::read_to_string(&path).context("failed to read project config.toml")?;
    parse_global_config(&contents)
}

/// Atomically write config.toml.
pub fn write_global(home: &Path, config: &GlobalConfig) -> Result<()> {
    let path = paths::config_path(home);
    let dir = path
        .parent()
        .ok_or_else(|| anyhow!("config path has no parent directory"))?;
    fs::create_dir_all(dir).context("failed to create config directory")?;
    let output = toml::to_string_pretty(config).context("failed to serialize config.toml")?;
    let mut tmp =
        tempfile::NamedTempFile::new_in(dir).context("failed to create temp file for config")?;
    tmp.write_all(output.as_bytes())
        .context("failed to write config temp file")?;
    tmp.as_file()
        .sync_all()
        .context("failed to sync config to disk")?;
    tmp.persist(&path)
        .context("failed to persist config file")?;
    Ok(())
}

fn parse_global_config(contents: &str) -> Result<GlobalConfig> {
    toml::from_str(contents).context("failed to parse config.toml")
}

/// Resolve a config value for a package using layered resolution.
/// Priority: flags > env > project per-pkg > project [zr] > receipt config > global per-pkg > global [zr] > package defaults
pub fn resolve(
    key: &str,
    package_name: &str,
    flags: &BTreeMap<String, String>,
    receipt: &Receipt,
    global_config: &GlobalConfig,
    package_config: &BTreeMap<String, serde_yml::Value>,
    project_config: Option<&GlobalConfig>,
) -> Option<String> {
    // 1. Flags (parsed CLI args)
    if let Some(v) = flags.get(key) {
        return Some(v.clone());
    }

    // 2. Environment variables (declaration-gated)
    if package_config.contains_key(key) {
        let env_var = config_env_var_name(package_name, key);
        if let Ok(v) = std::env::var(&env_var) {
            return Some(v);
        }
    }

    // 3. Project per-package config
    if let Some(proj) = project_config {
        if let Some(section) = proj.packages.get(package_name)
            && let Some(v) = section.get(key)
        {
            return Some(v.clone());
        }

        // 4. Project [zr] global config
        if let Some(v) = proj.zr.get(key) {
            return Some(v.clone());
        }
    }

    // 5. Receipt config
    if let Some(v) = receipt.config.get(key) {
        return Some(v.clone());
    }

    // 6. Global config.toml per-package
    if let Some(section) = global_config.packages.get(package_name)
        && let Some(v) = section.get(key)
    {
        return Some(v.clone());
    }

    // 7. Global config.toml [zr]
    if let Some(v) = global_config.zr.get(key) {
        return Some(v.clone());
    }

    // 8. Package defaults
    if let Some(v) = package_config.get(key) {
        return Some(yaml_value_to_string(v));
    }

    None
}

/// Build the `ZR_<PACKAGE>__<KEY>` environment variable name.
/// Hyphens → single underscores, everything uppercase.
pub fn config_env_var_name(package_name: &str, key: &str) -> String {
    let package_part = package_name.replace('-', "_").to_uppercase();
    let key_part = key.replace('-', "_").to_uppercase();
    format!("ZR_{}__{}", package_part, key_part)
}

/// Convert a serde_yml::Value to a string for config resolution.
pub fn yaml_value_to_string(v: &serde_yml::Value) -> String {
    match v {
        serde_yml::Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        serde_yml::Value::Number(n) => n.to_string(),
        serde_yml::Value::String(s) => s.clone(),
        _ => format!("{:?}", v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_definition;
    use crate::receipt::SourceRecord;
    use crate::test_util;

    fn make_package_config() -> BTreeMap<String, serde_yml::Value> {
        let yaml = r#"
name: my-pkg
version: "1.0.0"
binary: my-pkg
config:
  model: base
  language: auto
commands:
  default:
    description: Transcribe
"#;
        package_definition::parse(yaml).unwrap().config
    }

    fn make_receipt() -> Receipt {
        Receipt::new(
            "1.0.0".to_string(),
            SourceRecord::Local {
                path: "/tmp/test".to_string(),
            },
        )
    }

    #[test]
    fn test_config_layering_default() {
        let pkg_config = make_package_config();
        let receipt = make_receipt();
        let global = GlobalConfig::default();
        let flags = BTreeMap::new();

        let val = resolve(
            "model",
            "my-pkg",
            &flags,
            &receipt,
            &global,
            &pkg_config,
            None,
        );
        assert_eq!(val, Some("base".to_string()));
    }

    #[test]
    fn test_config_layering_receipt_overrides_default() {
        let pkg_config = make_package_config();
        let mut receipt = make_receipt();
        receipt.set_config("model".to_string(), "large".to_string());
        let global = GlobalConfig::default();
        let flags = BTreeMap::new();

        let val = resolve(
            "model",
            "my-pkg",
            &flags,
            &receipt,
            &global,
            &pkg_config,
            None,
        );
        assert_eq!(val, Some("large".to_string()));
    }

    #[test]
    fn test_config_layering_flag_overrides_all() {
        let pkg_config = make_package_config();
        let mut receipt = make_receipt();
        receipt.set_config("model".to_string(), "large".to_string());
        let global = GlobalConfig::default();
        let mut flags = BTreeMap::new();
        flags.insert("model".to_string(), "tiny".to_string());

        let val = resolve(
            "model",
            "my-pkg",
            &flags,
            &receipt,
            &global,
            &pkg_config,
            None,
        );
        assert_eq!(val, Some("tiny".to_string()));
    }

    #[test]
    fn test_config_layering_global_config() {
        let pkg_config = make_package_config();
        let receipt = make_receipt();
        let mut global = GlobalConfig::default();
        let mut section = BTreeMap::new();
        section.insert("model".to_string(), "medium".to_string());
        global.packages.insert("my-pkg".to_string(), section);
        let flags = BTreeMap::new();

        let val = resolve(
            "model",
            "my-pkg",
            &flags,
            &receipt,
            &global,
            &pkg_config,
            None,
        );
        assert_eq!(val, Some("medium".to_string()));
    }

    #[test]
    fn test_env_var_naming() {
        assert_eq!(
            config_env_var_name("my-tool", "output-dir"),
            "ZR_MY_TOOL__OUTPUT_DIR"
        );
        assert_eq!(config_env_var_name("my-pkg", "model"), "ZR_MY_PKG__MODEL");
    }

    #[test]
    fn test_env_var_declaration_gated() {
        let pkg_config = make_package_config();
        let receipt = make_receipt();
        let global = GlobalConfig::default();
        let flags = BTreeMap::new();

        // Set env var for undeclared key — should not be resolved
        unsafe {
            std::env::set_var("ZR_MY_PKG__UNDECLARED", "test");
        }
        let val = resolve(
            "undeclared",
            "my-pkg",
            &flags,
            &receipt,
            &global,
            &pkg_config,
            None,
        );
        assert!(val.is_none());
        unsafe {
            std::env::remove_var("ZR_MY_PKG__UNDECLARED");
        }
    }

    #[test]
    fn test_config_unset() {
        let mut receipt = make_receipt();
        receipt.set_config("model".to_string(), "large".to_string());
        receipt.unset_config("model");
        assert!(!receipt.config.contains_key("model"));
    }

    #[test]
    fn test_missing_config_file() {
        let home = test_util::temp_home("config");
        let config = read_global(home.path()).unwrap();
        assert!(config.zr.is_empty());
        assert!(config.packages.is_empty());
    }

    #[test]
    fn test_config_key_validation() {
        assert!(crate::platform::validate_config_key("model").is_ok());
        assert!(crate::platform::validate_config_key("output-format").is_ok());
        assert!(crate::platform::validate_config_key("output_format").is_err());
        assert!(crate::platform::validate_config_key("Model").is_err());
    }

    #[test]
    fn test_global_config_read_write() {
        let home = test_util::temp_home("config");
        let mut config = GlobalConfig::default();
        config.zr.insert("verbose".to_string(), "true".to_string());
        let mut pkg_section = BTreeMap::new();
        pkg_section.insert("model".to_string(), "large".to_string());
        config.packages.insert("my-pkg".to_string(), pkg_section);

        write_global(home.path(), &config).unwrap();
        let loaded = read_global(home.path()).unwrap();
        assert_eq!(loaded.zr.get("verbose").unwrap(), "true");
        assert_eq!(
            loaded.packages.get("my-pkg").unwrap().get("model").unwrap(),
            "large"
        );
    }

    #[test]
    fn test_project_config_overrides_global() {
        let pkg_config = make_package_config();
        let receipt = make_receipt();
        let mut global = GlobalConfig::default();
        let mut section = BTreeMap::new();
        section.insert("model".to_string(), "sha512".to_string());
        global.packages.insert("my-pkg".to_string(), section);
        let flags = BTreeMap::new();

        let mut project = GlobalConfig::default();
        let mut proj_section = BTreeMap::new();
        proj_section.insert("model".to_string(), "md5".to_string());
        project.packages.insert("my-pkg".to_string(), proj_section);

        let val = resolve(
            "model",
            "my-pkg",
            &flags,
            &receipt,
            &global,
            &pkg_config,
            Some(&project),
        );
        assert_eq!(val, Some("md5".to_string()));
    }

    #[test]
    fn test_project_config_overrides_receipt() {
        let pkg_config = make_package_config();
        let mut receipt = make_receipt();
        receipt.set_config("model".to_string(), "sha256".to_string());
        let global = GlobalConfig::default();
        let flags = BTreeMap::new();

        let mut project = GlobalConfig::default();
        let mut proj_section = BTreeMap::new();
        proj_section.insert("model".to_string(), "md5".to_string());
        project.packages.insert("my-pkg".to_string(), proj_section);

        let val = resolve(
            "model",
            "my-pkg",
            &flags,
            &receipt,
            &global,
            &pkg_config,
            Some(&project),
        );
        assert_eq!(val, Some("md5".to_string()));
    }

    #[test]
    fn test_flags_override_project_config() {
        let pkg_config = make_package_config();
        let receipt = make_receipt();
        let global = GlobalConfig::default();
        let mut flags = BTreeMap::new();
        flags.insert("model".to_string(), "sha512".to_string());

        let mut project = GlobalConfig::default();
        let mut proj_section = BTreeMap::new();
        proj_section.insert("model".to_string(), "md5".to_string());
        project.packages.insert("my-pkg".to_string(), proj_section);

        let val = resolve(
            "model",
            "my-pkg",
            &flags,
            &receipt,
            &global,
            &pkg_config,
            Some(&project),
        );
        assert_eq!(val, Some("sha512".to_string()));
    }

    #[test]
    fn test_absent_project_config_skips_layer() {
        let pkg_config = make_package_config();
        let mut receipt = make_receipt();
        receipt.set_config("model".to_string(), "large".to_string());
        let global = GlobalConfig::default();
        let flags = BTreeMap::new();

        let val = resolve(
            "model",
            "my-pkg",
            &flags,
            &receipt,
            &global,
            &pkg_config,
            None,
        );
        assert_eq!(val, Some("large".to_string()));
    }

    #[test]
    fn test_registries_read_write() {
        let home = test_util::temp_home("config");
        let mut config = GlobalConfig::default();
        config.registries.push(RegistryConfig {
            name: "default".to_string(),
            url: "https://github.com/zacor-packages/registry".to_string(),
        });
        config.registries.push(RegistryConfig {
            name: "company".to_string(),
            url: "https://github.com/my-org/zr-packages".to_string(),
        });

        write_global(home.path(), &config).unwrap();
        let loaded = read_global(home.path()).unwrap();
        assert_eq!(loaded.registries.len(), 2);
        assert_eq!(loaded.registries[0].name, "default");
        assert_eq!(
            loaded.registries[0].url,
            "https://github.com/zacor-packages/registry"
        );
        assert_eq!(loaded.registries[1].name, "company");
    }

    #[test]
    fn test_config_without_registries_parses() {
        let home = test_util::temp_home("config");
        let mut config = GlobalConfig::default();
        config.zr.insert("verbose".to_string(), "true".to_string());
        write_global(home.path(), &config).unwrap();

        let loaded = read_global(home.path()).unwrap();
        assert!(loaded.registries.is_empty());
        assert_eq!(loaded.zr.get("verbose").unwrap(), "true");
    }

    #[test]
    fn test_mixed_config_with_registries() {
        let home = test_util::temp_home("config");
        let mut config = GlobalConfig::default();
        config.zr.insert("verbose".to_string(), "true".to_string());
        let mut pkg_section = BTreeMap::new();
        pkg_section.insert("model".to_string(), "large".to_string());
        config.packages.insert("my-pkg".to_string(), pkg_section);
        config.registries.push(RegistryConfig {
            name: "default".to_string(),
            url: "https://example.com/registry".to_string(),
        });

        write_global(home.path(), &config).unwrap();
        let loaded = read_global(home.path()).unwrap();
        assert_eq!(loaded.zr.get("verbose").unwrap(), "true");
        assert_eq!(
            loaded.packages.get("my-pkg").unwrap().get("model").unwrap(),
            "large"
        );
        assert_eq!(loaded.registries.len(), 1);
        assert_eq!(loaded.registries[0].name, "default");
    }
}
