use crate::error::*;
use crate::receipt;
use std::path::Path;

pub fn run(home: &Path, features: &[String]) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;

    let resolution = resolve_features(&cwd, features).map_err(|e| anyhow::anyhow!(e))?;
    if let Some(detected) = &resolution.detected {
        eprintln!("Detected features: {}", detected.join(", "));
    } else {
        eprintln!("Selected features: {}", resolution.features.join(", "));
    }

    // Create .zr/ project root
    let zr_dir = cwd.join(".zr");
    if !zr_dir.exists() {
        std::fs::create_dir_all(&zr_dir).context("failed to create .zr/")?;
        eprintln!("Created .zr/");
    }

    // Scan installed packages for those with an `init` command
    let packages = receipt::list_all(home).context("failed to list packages")?;
    let mut dispatched = 0;
    let mut failed = 0;

    if resolution.features.is_empty() {
        eprintln!("No supported features detected. Nothing to sync.");
        return Ok(());
    }

    let features_csv = resolution.features.join(",");
    for (name, r) in &packages {
        if !r.active {
            continue;
        }
        let def = match crate::wasm_manifest::load_from_store(home, name, &r.current) {
            Ok(d) => d,
            Err(_) => continue,
        };

        if !def.commands.contains_key("init") {
            continue;
        }

        eprint!("  {name}... ");
        let status = std::process::Command::new("zr")
            .args(package_init_args(name, &features_csv))
            .status();

        match status {
            Ok(s) if s.success() => {
                eprintln!("ok");
                dispatched += 1;
            }
            Ok(s) => {
                eprintln!("failed (exit {})", s.code().unwrap_or(-1));
                failed += 1;
            }
            Err(e) => {
                eprintln!("failed ({e})");
                failed += 1;
            }
        }
    }

    eprintln!(
        "\nFeatures synced: [{}]\nPackages dispatched: {dispatched}",
        resolution.features.join(", "),
    );
    if failed > 0 {
        bail!("{failed} package(s) failed during init");
    }
    Ok(())
}

#[derive(Debug)]
struct FeatureResolution {
    features: Vec<String>,
    detected: Option<Vec<String>>,
}

fn resolve_features(
    root: &Path,
    requested: &[String],
) -> std::result::Result<FeatureResolution, String> {
    if requested.is_empty() {
        let detected = detect_features(root);
        return Ok(FeatureResolution {
            features: detected.clone(),
            detected: Some(detected),
        });
    }

    zacor_package::skills::validate_features(requested)?;
    Ok(FeatureResolution {
        features: requested.to_vec(),
        detected: None,
    })
}

fn detect_features(root: &Path) -> Vec<String> {
    zacor_package::skills::FEATURES
        .iter()
        .filter(|feature| root.join(feature.dir).is_dir())
        .map(|feature| feature.name.to_string())
        .collect()
}

fn package_init_args<'a>(package_name: &'a str, features_csv: &'a str) -> [&'a str; 3] {
    [package_name, "init", features_csv]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn resolve_explicit_features_validates_and_preserves_order() {
        let tmp = TempDir::new().unwrap();
        let resolved =
            resolve_features(tmp.path(), &["gemini".into(), "claude-code".into()]).unwrap();
        assert_eq!(resolved.features, vec!["gemini", "claude-code"]);
        assert!(resolved.detected.is_none());
    }

    #[test]
    fn resolve_explicit_features_rejects_unknown_names() {
        let tmp = TempDir::new().unwrap();
        let err = resolve_features(tmp.path(), &["vscode".into()]).unwrap_err();
        assert!(err.contains("Unknown feature"));
        assert!(err.contains("claude-code"));
    }

    #[test]
    fn resolve_auto_detects_supported_feature_directories() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
        std::fs::create_dir_all(tmp.path().join(".codex")).unwrap();

        let resolved = resolve_features(tmp.path(), &[]).unwrap();
        assert_eq!(resolved.features, vec!["claude-code", "codex"]);
        assert_eq!(
            resolved.detected,
            Some(vec!["claude-code".into(), "codex".into()])
        );
    }

    #[test]
    fn resolve_auto_returns_empty_when_no_features_are_detected() {
        let tmp = TempDir::new().unwrap();
        let resolved = resolve_features(tmp.path(), &[]).unwrap();
        assert!(resolved.features.is_empty());
        assert_eq!(resolved.detected, Some(Vec::new()));
    }

    #[test]
    fn package_dispatch_uses_positional_features_argument() {
        assert_eq!(
            package_init_args("wf", "claude-code,gemini"),
            ["wf", "init", "claude-code,gemini"]
        );
    }
}
