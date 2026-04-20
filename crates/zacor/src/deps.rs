use crate::error::*;
use crate::package_definition::PackageDefinition;
use crate::receipt;
use std::path::Path;

pub struct UnsatisfiedPackage {
    pub name: String,
    pub installed: bool,
}

pub struct UnsatisfiedBinary {
    pub name: String,
    pub hint: Option<String>,
}

/// Check if all package dependencies are satisfied (installed and active).
pub fn check_package_deps(home: &Path, def: &PackageDefinition) -> Result<Vec<UnsatisfiedPackage>> {
    let mut missing = Vec::new();
    for dep in &def.depends.packages {
        match receipt::read(home, &dep.name)? {
            Some(r) if r.active => {}
            Some(_) => missing.push(UnsatisfiedPackage {
                name: dep.name.clone(),
                installed: true,
            }),
            None => missing.push(UnsatisfiedPackage {
                name: dep.name.clone(),
                installed: false,
            }),
        }
    }
    Ok(missing)
}

/// Check if all external binary dependencies are available.
pub fn check_binary_deps(def: &PackageDefinition) -> Vec<UnsatisfiedBinary> {
    let mut missing = Vec::new();
    for dep in &def.depends.binaries {
        if which::which(&dep.binary).is_err() {
            missing.push(UnsatisfiedBinary {
                name: dep.binary.clone(),
                hint: dep.install_hint.clone(),
            });
        }
    }
    missing
}

/// Scan all installed packages for reverse dependencies on the given package name.
pub fn find_reverse_deps(home: &Path, package_name: &str) -> Result<Vec<String>> {
    let mut dependents = Vec::new();
    let all = receipt::list_all(home)?;
    for (name, r) in &all {
        if name == package_name {
            continue;
        }
        if let Ok(def) = crate::wasm_manifest::load_from_store(home, name, &r.current)
            && def.depends.packages.iter().any(|d| d.name == package_name)
        {
            dependents.push(name.clone());
        }
    }
    Ok(dependents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_definition;
    use crate::receipt::{Receipt, SourceRecord};
    use crate::store;
    use crate::test_util;

    fn local_source() -> SourceRecord {
        SourceRecord::Local {
            path: "/tmp/test".to_string(),
        }
    }

    #[test]
    fn test_check_package_deps_satisfied() {
        let home = test_util::temp_home("deps");
        // Install dependency
        let dep_receipt = Receipt::new("1.0.0".to_string(), local_source());
        receipt::write(home.path(), "my-pkg", &dep_receipt).unwrap();

        let yaml = r#"
name: my-tool
version: "1.0.0"
commands:
  default:
    description: test
depends:
  packages:
    - name: my-pkg
"#;
        let def = package_definition::parse(yaml).unwrap();
        let missing = check_package_deps(home.path(), &def).unwrap();
        assert!(missing.is_empty());
    }

    #[test]
    fn test_check_package_deps_missing() {
        let home = test_util::temp_home("deps");
        let yaml = r#"
name: my-tool
version: "1.0.0"
commands:
  default:
    description: test
depends:
  packages:
    - name: my-pkg
"#;
        let def = package_definition::parse(yaml).unwrap();
        let missing = check_package_deps(home.path(), &def).unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].name, "my-pkg");
        assert!(!missing[0].installed);
    }

    #[test]
    fn test_reverse_dep_scan() {
        let home = test_util::temp_home("deps");

        // Install package A with no deps
        let ra = Receipt::new("1.0.0".to_string(), local_source());
        receipt::write(home.path(), "pkg-a", &ra).unwrap();
        let def_a = "name: pkg-a\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: a\n";
        let content_a = tempfile::tempdir().unwrap();
        std::fs::write(content_a.path().join("package.yaml"), def_a).unwrap();
        store::put(home.path(), "pkg-a", "1.0.0", content_a.path()).unwrap();

        // Install package B that depends on A
        let rb = Receipt::new("1.0.0".to_string(), local_source());
        receipt::write(home.path(), "pkg-b", &rb).unwrap();
        let def_b = "name: pkg-b\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: b\ndepends:\n  packages:\n    - name: pkg-a\n";
        let content_b = tempfile::tempdir().unwrap();
        std::fs::write(content_b.path().join("package.yaml"), def_b).unwrap();
        store::put(home.path(), "pkg-b", "1.0.0", content_b.path()).unwrap();

        let dependents = find_reverse_deps(home.path(), "pkg-a").unwrap();
        assert_eq!(dependents, vec!["pkg-b"]);
    }

    #[test]
    fn test_circular_ref_doesnt_crash() {
        let home = test_util::temp_home("deps");
        // Package A depends on B, B depends on A — should not crash
        let r = Receipt::new("1.0.0".to_string(), local_source());
        receipt::write(home.path(), "circ-a", &r).unwrap();
        receipt::write(home.path(), "circ-b", &r).unwrap();

        let def_a = "name: circ-a\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: a\ndepends:\n  packages:\n    - name: circ-b\n";
        let def_b = "name: circ-b\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: b\ndepends:\n  packages:\n    - name: circ-a\n";
        let content_a = tempfile::tempdir().unwrap();
        let content_b = tempfile::tempdir().unwrap();
        std::fs::write(content_a.path().join("package.yaml"), def_a).unwrap();
        std::fs::write(content_b.path().join("package.yaml"), def_b).unwrap();
        store::put(home.path(), "circ-a", "1.0.0", content_a.path()).unwrap();
        store::put(home.path(), "circ-b", "1.0.0", content_b.path()).unwrap();

        // Should find circ-b as a dependent of circ-a
        let deps = find_reverse_deps(home.path(), "circ-a").unwrap();
        assert_eq!(deps, vec!["circ-b"]);
    }
}
