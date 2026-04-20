//! Read the embedded package manifest from a wasm artifact.
//!
//! Packages built with `zacor-package-build` embed their generated
//! `package.yaml` bytes in a wasm custom section named `zacor_manifest`.
//! This module walks a `.wasm` file's sections to extract those bytes
//! so the host can discover a package's schema without requiring a
//! sidecar yaml on disk.

use crate::error::*;
use crate::package_definition::{self, PackageDefinition};
use crate::paths;
use std::path::{Path, PathBuf};
use wasmparser::{Parser, Payload};

/// The custom section name produced by
/// `zacor_package_build::generate_embedded_manifest`.
pub const MANIFEST_SECTION: &str = "zacor_manifest";

/// Read the raw manifest yaml bytes from a wasm artifact's custom
/// section. Returns `Ok(None)` if the section is absent — valid
/// wasm without an embedded manifest.
pub fn read_manifest_bytes(wasm_path: &Path) -> Result<Option<Vec<u8>>> {
    let bytes = std::fs::read(wasm_path)
        .with_context(|| format!("failed to read wasm file {}", wasm_path.display()))?;
    read_manifest_bytes_from_slice(&bytes)
}

/// Same as `read_manifest_bytes`, but takes in-memory wasm bytes.
pub fn read_manifest_bytes_from_slice(wasm: &[u8]) -> Result<Option<Vec<u8>>> {
    for payload in Parser::new(0).parse_all(wasm) {
        let payload = payload.context("parsing wasm")?;
        if let Payload::CustomSection(reader) = payload {
            if reader.name() == MANIFEST_SECTION {
                return Ok(Some(reader.data().to_vec()));
            }
        }
    }
    Ok(None)
}

/// Extract and parse the embedded manifest from a wasm artifact.
/// Returns `Err` if the section is missing or the yaml is invalid.
pub fn read_manifest(wasm_path: &Path) -> Result<PackageDefinition> {
    let bytes = read_manifest_bytes(wasm_path)?.ok_or_else(|| {
        anyhow!(
            "wasm artifact {} has no embedded '{}' custom section\n\
             hint: rebuild with zacor-package-build so the manifest is embedded",
            wasm_path.display(),
            MANIFEST_SECTION
        )
    })?;
    let yaml = std::str::from_utf8(&bytes).context("manifest is not valid UTF-8")?;
    package_definition::parse(yaml).context("parsing embedded manifest yaml")
}

/// Locate a wasm artifact in a store directory. Scans for any `*.wasm`
/// file; by convention a package version stores at most one. Returns
/// `Ok(None)` if none present (native package) or the directory doesn't
/// exist.
pub fn find_wasm_in_store(dir: &Path) -> Result<Option<PathBuf>> {
    if !dir.is_dir() {
        return Ok(None);
    }
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry.context("reading store directory entry")?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("wasm") {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// Load a package's manifest from its store directory.
///
/// Wasm packages carry the manifest inside the `.wasm` artifact as a
/// custom section — that's the source of truth when present. Native
/// packages fall back to the sidecar `package.yaml`. Callers don't
/// need to know which kind the package is.
pub fn load_from_store(home: &Path, name: &str, version: &str) -> Result<PackageDefinition> {
    let dir = paths::store_path(home, name, version);
    if let Some(wasm_path) = find_wasm_in_store(&dir)? {
        return read_manifest(&wasm_path).with_context(|| {
            format!(
                "reading embedded manifest from {}",
                wasm_path.display()
            )
        });
    }
    let yaml_path = paths::definition_path(home, name, version);
    package_definition::parse_file(&yaml_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo_wasm_path() -> Option<std::path::PathBuf> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent()?.parent()?;
        let p = workspace_root
            .join("target")
            .join("wasm32-wasip1")
            .join("release")
            .join("echo.wasm");
        if p.exists() {
            Some(p)
        } else {
            None
        }
    }

    #[test]
    fn echo_wasm_carries_embedded_manifest() {
        let Some(path) = echo_wasm_path() else {
            eprintln!("skipping: build zr-echo for wasm32-wasip1 first");
            return;
        };

        let bytes = read_manifest_bytes(&path).expect("read");
        let bytes = bytes.expect("echo.wasm should carry a zacor_manifest custom section");
        let yaml = std::str::from_utf8(&bytes).expect("utf-8");

        assert!(
            yaml.contains("name: echo"),
            "manifest should declare name: echo, got: {}",
            yaml
        );
        assert!(
            yaml.contains("protocol: true"),
            "manifest should declare protocol: true, got: {}",
            yaml
        );
        assert!(
            yaml.contains("default:"),
            "manifest should declare a default command, got: {}",
            yaml
        );
    }

    #[test]
    fn parse_embedded_manifest_matches_package() {
        let Some(path) = echo_wasm_path() else {
            eprintln!("skipping: build zr-echo for wasm32-wasip1 first");
            return;
        };

        let def = read_manifest(&path).expect("parse embedded manifest");
        assert_eq!(def.name, "echo");
        assert_eq!(def.version, "0.2.0");
        assert!(def.commands.contains_key("default"));
    }
}
