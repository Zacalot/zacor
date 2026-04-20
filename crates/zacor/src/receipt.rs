use crate::error::*;
use crate::paths;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::Path;
use tempfile::NamedTempFile;

const CURRENT_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Command,
    Service,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Command => write!(f, "command"),
            Mode::Service => write!(f, "service"),
        }
    }
}

impl std::str::FromStr for Mode {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "command" => Ok(Mode::Command),
            "service" => Ok(Mode::Service),
            _ => Err(anyhow!("unknown mode '{}' (expected: command, service)", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Local,
    Remote,
}

impl std::fmt::Display for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Transport::Local => write!(f, "local"),
            Transport::Remote => write!(f, "remote"),
        }
    }
}

impl std::str::FromStr for Transport {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "local" => Ok(Transport::Local),
            "remote" => Ok(Transport::Remote),
            _ => Err(anyhow!(
                "unknown transport '{}' (expected: local, remote)",
                s
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub schema: u32,
    pub current: String,
    pub active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<Mode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<Transport>,
    #[serde(default)]
    pub config: BTreeMap<String, String>,
    pub versions: BTreeMap<String, VersionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionEntry {
    pub source: SourceRecord,
    pub installed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SourceRecord {
    Local {
        path: String,
    },
    Github {
        owner: String,
        repo: String,
        tag: String,
        asset: String,
    },
    Registry {
        registry: String,
        package: String,
        version: String,
    },
}

impl Receipt {
    pub fn new(version: String, source: SourceRecord) -> Self {
        let mut versions = BTreeMap::new();
        versions.insert(
            version.clone(),
            VersionEntry {
                source,
                installed_at: Utc::now(),
            },
        );
        Receipt {
            schema: CURRENT_SCHEMA,
            current: version,
            active: true,
            mode: None,
            transport: None,
            config: BTreeMap::new(),
            versions,
        }
    }

    /// Add a version and set it as current.
    pub fn add_version(&mut self, version: String, source: SourceRecord) {
        self.versions.insert(
            version.clone(),
            VersionEntry {
                source,
                installed_at: Utc::now(),
            },
        );
        self.current = version;
    }

    /// Remove a version. Returns true if the receipt should be deleted (no versions left).
    pub fn remove_version(&mut self, version: &str) -> bool {
        self.versions.remove(version);
        if self.versions.is_empty() {
            return true;
        }
        if self.current == version {
            self.current = highest_version(self.versions.keys());
        }
        false
    }

    /// Validate that a version exists, returning a formatted error with available versions if not.
    pub fn require_version(&self, name: &str, version: &str) -> Result<()> {
        if self.versions.contains_key(version) {
            return Ok(());
        }
        let available: Vec<&str> = self.versions.keys().map(|s| s.as_str()).collect();
        bail!(
            "version '{}' not found for '{}'\navailable versions: {}",
            version,
            name,
            available.join(", ")
        );
    }

    pub fn set_config(&mut self, key: String, value: String) {
        self.config.insert(key, value);
    }

    pub fn unset_config(&mut self, key: &str) {
        self.config.remove(key);
    }

    #[cfg(test)]
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = Some(mode);
    }

    #[cfg(test)]
    pub fn set_transport(&mut self, transport: Transport) {
        self.transport = Some(transport);
    }
}

/// Read a receipt or return an error if the package is not found.
pub fn require(home: &Path, name: &str) -> Result<Receipt> {
    read(home, name)?.ok_or_else(|| anyhow!("package '{}' not found", name))
}

/// Read a receipt from disk. Returns None if the file doesn't exist.
pub fn read(home: &Path, name: &str) -> Result<Option<Receipt>> {
    let path = paths::receipt_path(home, name);
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read receipt for '{}'", name))?;
    let receipt: Receipt = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse receipt for '{}'", name))?;
    if receipt.schema > CURRENT_SCHEMA {
        bail!(
            "receipt for '{}' was created by a newer version of zacor (schema {}), please upgrade",
            name,
            receipt.schema
        );
    }
    Ok(Some(receipt))
}

/// Atomically write a receipt to disk.
pub fn write(home: &Path, name: &str, receipt: &Receipt) -> Result<()> {
    let path = paths::receipt_path(home, name);
    let dir = paths::modules_dir(home);
    fs::create_dir_all(&dir).context("failed to create modules directory")?;
    let contents = serde_json::to_string_pretty(receipt).context("failed to serialize receipt")?;
    let mut tmp = NamedTempFile::new_in(&dir).context("failed to create temp file for receipt")?;
    tmp.write_all(contents.as_bytes())
        .context("failed to write receipt temp file")?;
    tmp.as_file()
        .sync_all()
        .context("failed to sync receipt to disk")?;
    tmp.persist(&path)
        .context("failed to persist receipt file")?;
    Ok(())
}

/// Delete a receipt file.
pub fn delete(home: &Path, name: &str) -> Result<()> {
    let path = paths::receipt_path(home, name);
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("failed to delete receipt for '{}'", name))?;
    }
    Ok(())
}

/// List all installed modules by reading receipt files from modules/.
pub fn list_all(home: &Path) -> Result<Vec<(String, Receipt)>> {
    let dir = paths::modules_dir(home);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut results = Vec::new();
    for entry in fs::read_dir(&dir).context("failed to read modules directory")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue;
            }
            let contents = fs::read_to_string(&path)?;
            if let Ok(receipt) = serde_json::from_str::<Receipt>(&contents) {
                results.push((name, receipt));
            }
        }
    }
    results.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(results)
}

/// Find the highest version from a set of version strings using semver comparison.
fn highest_version<'a, I>(versions: I) -> String
where
    I: Iterator<Item = &'a String>,
{
    let mut best: Option<(&str, Option<semver::Version>)> = None;
    for v in versions {
        let parsed = semver::Version::parse(v).ok();
        match (&best, &parsed) {
            (None, _) => best = Some((v, parsed)),
            (Some((_, None)), Some(_)) => best = Some((v, parsed)),
            (Some((_, Some(current))), Some(candidate)) if candidate > current => {
                best = Some((v, parsed));
            }
            (Some((_, None)), None) => {
                // Both non-semver: lexicographic
                if v.as_str() > best.as_ref().unwrap().0 {
                    best = Some((v, parsed));
                }
            }
            _ => {}
        }
    }
    best.map(|(v, _)| v.to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util;

    fn local_source() -> SourceRecord {
        SourceRecord::Local {
            path: "/tmp/tool.tar.gz".to_string(),
        }
    }

    fn github_source() -> SourceRecord {
        SourceRecord::Github {
            owner: "user".to_string(),
            repo: "tool".to_string(),
            tag: "v1.0.0".to_string(),
            asset: "tool-linux-amd64.tar.gz".to_string(),
        }
    }

    #[test]
    fn test_create_and_read_receipt() {
        let home = test_util::temp_home("receipt");
        let receipt = Receipt::new("1.0.0".to_string(), local_source());
        write(home.path(), "test-tool", &receipt).unwrap();
        let loaded = read(home.path(), "test-tool").unwrap().unwrap();
        assert_eq!(loaded.schema, 1);
        assert_eq!(loaded.current, "1.0.0");
        assert!(loaded.active);
        assert_eq!(loaded.mode, None);
        assert_eq!(loaded.transport, None);
        assert!(loaded.versions.contains_key("1.0.0"));
    }

    #[test]
    fn test_read_missing_receipt() {
        let home = test_util::temp_home("receipt");
        let result = read(home.path(), "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_add_version() {
        let home = test_util::temp_home("receipt");
        let mut receipt = Receipt::new("1.0.0".to_string(), local_source());
        receipt.add_version("2.0.0".to_string(), github_source());
        assert_eq!(receipt.current, "2.0.0");
        assert_eq!(receipt.versions.len(), 2);
        write(home.path(), "test-tool", &receipt).unwrap();
        let loaded = read(home.path(), "test-tool").unwrap().unwrap();
        assert_eq!(loaded.versions.len(), 2);
    }

    #[test]
    fn test_remove_version_switches_to_highest() {
        let mut receipt = Receipt::new("2.0.0".to_string(), local_source());
        receipt.add_version("13.0.0".to_string(), local_source());
        receipt.add_version("14.1.0".to_string(), local_source());
        assert_eq!(receipt.current, "14.1.0");
        let should_delete = receipt.remove_version("14.1.0");
        assert!(!should_delete);
        // Should pick 13.0.0 via semver, not 2.0.0 (lexicographic would pick 2.0.0)
        assert_eq!(receipt.current, "13.0.0");
    }

    #[test]
    fn test_remove_last_version() {
        let mut receipt = Receipt::new("1.0.0".to_string(), local_source());
        let should_delete = receipt.remove_version("1.0.0");
        assert!(should_delete);
    }

    #[test]
    fn test_remove_non_current_version() {
        let mut receipt = Receipt::new("1.0.0".to_string(), local_source());
        receipt.add_version("2.0.0".to_string(), local_source());
        let should_delete = receipt.remove_version("1.0.0");
        assert!(!should_delete);
        assert_eq!(receipt.current, "2.0.0"); // unchanged
    }

    #[test]
    fn test_config_set_unset() {
        let mut receipt = Receipt::new("1.0.0".to_string(), local_source());
        receipt.set_config("model".to_string(), "large".to_string());
        assert_eq!(receipt.config.get("model").unwrap(), "large");
        receipt.unset_config("model");
        assert!(!receipt.config.contains_key("model"));
        // Unsetting non-existent key is idempotent
        receipt.unset_config("nonexistent");
    }

    #[test]
    fn test_set_mode_transport() {
        let mut receipt = Receipt::new("1.0.0".to_string(), local_source());
        receipt.set_mode(Mode::Service);
        assert_eq!(receipt.mode, Some(Mode::Service));
        receipt.set_transport(Transport::Remote);
        assert_eq!(receipt.transport, Some(Transport::Remote));
    }

    #[test]
    fn test_atomic_write() {
        let home = test_util::temp_home("receipt");
        let receipt = Receipt::new("1.0.0".to_string(), local_source());
        write(home.path(), "test-tool", &receipt).unwrap();
        // Overwrite
        let mut receipt2 = receipt;
        receipt2.add_version("2.0.0".to_string(), local_source());
        write(home.path(), "test-tool", &receipt2).unwrap();
        let loaded = read(home.path(), "test-tool").unwrap().unwrap();
        assert_eq!(loaded.versions.len(), 2);
    }

    #[test]
    fn test_forward_compat_rejection() {
        let home = test_util::temp_home("receipt");
        let mut receipt = Receipt::new("1.0.0".to_string(), local_source());
        receipt.schema = 99;
        write(home.path(), "test-tool", &receipt).unwrap();
        let result = read(home.path(), "test-tool");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("newer version"), "got: {}", err);
        assert!(err.contains("upgrade"), "got: {}", err);
    }

    #[test]
    fn test_list_all() {
        let home = test_util::temp_home("receipt");
        write(
            home.path(),
            "alpha",
            &Receipt::new("1.0.0".to_string(), local_source()),
        )
        .unwrap();
        write(
            home.path(),
            "beta",
            &Receipt::new("2.0.0".to_string(), local_source()),
        )
        .unwrap();
        let list = list_all(home.path()).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].0, "alpha");
        assert_eq!(list[1].0, "beta");
    }

    #[test]
    fn test_list_all_empty() {
        let home = test_util::temp_home("receipt");
        let list = list_all(home.path()).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn test_delete_receipt() {
        let home = test_util::temp_home("receipt");
        let receipt = Receipt::new("1.0.0".to_string(), local_source());
        write(home.path(), "test-tool", &receipt).unwrap();
        assert!(paths::receipt_path(home.path(), "test-tool").exists());
        delete(home.path(), "test-tool").unwrap();
        assert!(!paths::receipt_path(home.path(), "test-tool").exists());
    }

    #[test]
    fn test_require_found() {
        let home = test_util::temp_home("receipt");
        let receipt = Receipt::new("1.0.0".to_string(), local_source());
        write(home.path(), "test-tool", &receipt).unwrap();
        let loaded = require(home.path(), "test-tool").unwrap();
        assert_eq!(loaded.current, "1.0.0");
    }

    #[test]
    fn test_require_not_found() {
        let home = test_util::temp_home("receipt");
        let result = require(home.path(), "nonexistent");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"), "got: {}", err);
    }

    #[test]
    fn test_require_version_found() {
        let receipt = Receipt::new("1.0.0".to_string(), local_source());
        assert!(receipt.require_version("pkg", "1.0.0").is_ok());
    }

    #[test]
    fn test_require_version_not_found() {
        let mut receipt = Receipt::new("1.0.0".to_string(), local_source());
        receipt.add_version("2.0.0".to_string(), local_source());
        let result = receipt.require_version("pkg", "3.0.0");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("3.0.0"), "got: {}", err);
        assert!(err.contains("pkg"), "got: {}", err);
        assert!(err.contains("1.0.0"), "got: {}", err);
        assert!(err.contains("2.0.0"), "got: {}", err);
    }

    #[test]
    fn test_semver_ordering() {
        let versions: Vec<String> = vec!["2.0.0".into(), "13.0.0".into(), "14.1.0".into()];
        assert_eq!(highest_version(versions.iter()), "14.1.0");
    }

    #[test]
    fn test_semver_beats_non_semver() {
        let versions: Vec<String> = vec!["local-abc".into(), "1.0.0".into()];
        assert_eq!(highest_version(versions.iter()), "1.0.0");
    }

    #[test]
    fn test_source_record_serialization() {
        let receipt = Receipt::new("1.0.0".to_string(), github_source());
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(json.contains("\"type\":\"github\""));
        let parsed: Receipt = serde_json::from_str(&json).unwrap();
        match &parsed.versions["1.0.0"].source {
            SourceRecord::Github { owner, repo, .. } => {
                assert_eq!(owner, "user");
                assert_eq!(repo, "tool");
            }
            _ => panic!("expected github source"),
        }
    }

    #[test]
    fn test_source_record_registry_roundtrip() {
        let source = SourceRecord::Registry {
            registry: "default".to_string(),
            package: "echo".to_string(),
            version: "0.2.0".to_string(),
        };
        let receipt = Receipt::new("0.2.0".to_string(), source);
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(json.contains("\"type\":\"registry\""), "got: {}", json);
        let parsed: Receipt = serde_json::from_str(&json).unwrap();
        match &parsed.versions["0.2.0"].source {
            SourceRecord::Registry {
                registry,
                package,
                version,
            } => {
                assert_eq!(registry, "default");
                assert_eq!(package, "echo");
                assert_eq!(version, "0.2.0");
            }
            _ => panic!("expected registry source"),
        }
    }

    #[test]
    fn test_old_receipts_still_parse() {
        // Simulate an old receipt with only Local and Github variants
        let json = r#"{"schema":1,"current":"1.0.0","active":true,"config":{},"versions":{"1.0.0":{"source":{"type":"local","path":"/tmp/tool.tar.gz"},"installed_at":"2024-01-01T00:00:00Z"}}}"#;
        let receipt: Receipt = serde_json::from_str(json).unwrap();
        assert_eq!(receipt.current, "1.0.0");
        match &receipt.versions["1.0.0"].source {
            SourceRecord::Local { path } => assert_eq!(path, "/tmp/tool.tar.gz"),
            _ => panic!("expected local source"),
        }
    }
}
