use crate::deps;
use crate::error::*;
use crate::package_definition::PackageDep;
use crate::platform;
use crate::receipt::{self, Receipt, SourceRecord};
use crate::source;
use crate::store;
use std::collections::HashSet;
use std::path::Path;

pub fn run(
    home: &Path,
    src: &str,
    name_override: Option<&str>,
    force: bool,
    inactive: bool,
    from_source: bool,
) -> Result<()> {
    let parsed = source::parse(src)?;
    let mut visiting = Vec::new();
    let mut installed = HashSet::new();
    install_recursive(
        home,
        &parsed,
        name_override,
        force,
        inactive,
        from_source,
        &mut visiting,
        &mut installed,
    )
}

fn install_recursive(
    home: &Path,
    src: &source::Source,
    name_override: Option<&str>,
    force: bool,
    inactive: bool,
    from_source: bool,
    visiting: &mut Vec<String>,
    installed: &mut HashSet<String>,
) -> Result<()> {
    let result = source::acquire(src, name_override, home, from_source)?;
    let package_name = result.name.clone();

    if let Some(index) = visiting.iter().position(|name| name == &package_name) {
        let mut cycle = visiting[index..].to_vec();
        cycle.push(package_name.clone());
        bail!("dependency cycle detected: {}", cycle.join(" -> "));
    }
    if installed.contains(&package_name) {
        return Ok(());
    }

    visiting.push(package_name.clone());
    for dep in &result.definition.depends.packages {
        match dependency_status(home, dep)? {
            DependencyStatus::Satisfied => continue,
            DependencyStatus::InstalledInactive => {
                bail!(
                    "dependency '{}' for '{}' is installed but inactive\nhint: enable it with `zacor enable {}`",
                    dep.name,
                    package_name,
                    dep.name
                );
            }
            DependencyStatus::VersionMismatch { installed } => {
                bail!(
                    "package '{}' requires '{}' {}, but '{}' is installed\nhint: upgrade with `zacor use {} {}` or remove with `--force`",
                    package_name,
                    dep.name,
                    dep.version.as_deref().unwrap_or("<unspecified>"),
                    installed,
                    dep.name,
                    dep.version.as_deref().unwrap_or("<version>")
                );
            }
            DependencyStatus::Missing => {
                let dep_source = resolve_dep_source(dep, &result.source_info)?;
                install_recursive(
                    home,
                    &dep_source,
                    None,
                    force,
                    false,
                    from_source,
                    visiting,
                    installed,
                )?;
            }
        }
    }

    let _ = visiting.pop();
    install_acquired(home, result, force, inactive)?;
    installed.insert(package_name);
    Ok(())
}

enum DependencyStatus {
    Satisfied,
    InstalledInactive,
    VersionMismatch { installed: String },
    Missing,
}

fn dependency_status(home: &Path, dep: &PackageDep) -> Result<DependencyStatus> {
    let Some(receipt) = receipt::read(home, &dep.name)? else {
        return Ok(DependencyStatus::Missing);
    };

    if let Some(required) = dep.version.as_deref()
        && !version_matches(&receipt.current, required)
    {
        return Ok(DependencyStatus::VersionMismatch {
            installed: receipt.current,
        });
    }

    if receipt.active {
        Ok(DependencyStatus::Satisfied)
    } else {
        Ok(DependencyStatus::InstalledInactive)
    }
}

fn version_matches(current: &str, requirement: &str) -> bool {
    let current = platform::normalize_version(current);
    if let (Ok(current), Ok(requirement)) = (
        semver::Version::parse(&current),
        semver::VersionReq::parse(requirement),
    ) {
        return requirement.matches(&current);
    }
    current == platform::normalize_version(requirement)
}

fn resolve_dep_source(dep: &PackageDep, parent_source: &SourceRecord) -> Result<source::Source> {
    if let Some(source) = dep.source.as_deref() {
        let parsed = source::parse(source)?;
        return Ok(match parsed {
            source::Source::Registry {
                name,
                version,
                registry,
            } => source::Source::Registry {
                name,
                version: version.or_else(|| dep.version.clone()),
                registry,
            },
            other => other,
        });
    }

    let registry = match parent_source {
        SourceRecord::Registry { registry, .. } => Some(registry.clone()),
        _ => None,
    };

    Ok(source::Source::Registry {
        name: dep.name.clone(),
        version: dep.version.clone(),
        registry,
    })
}

fn install_acquired(
    home: &Path,
    result: source::AcquireResult,
    force: bool,
    inactive: bool,
) -> Result<()> {
    let version = platform::normalize_version(&result.definition.version);

    let existing = receipt::read(home, &result.name)?;
    if let Some(ref receipt) = existing
        && receipt.versions.contains_key(&version)
        && !force
    {
        eprintln!("{} v{} is already installed", result.name, version);
        return Ok(());
    }

    store::put(home, &result.name, &version, result.content_dir.path())
        .context("failed to store package")?;

    let unsatisfied = deps::check_package_deps(home, &result.definition)?;
    let deps_satisfied = unsatisfied.is_empty();

    let mut receipt = match existing {
        Some(mut receipt) => {
            receipt.add_version(version.clone(), result.source_info);
            receipt
        }
        None => Receipt::new(version.clone(), result.source_info),
    };
    if !deps_satisfied || inactive {
        receipt.active = false;
    }

    receipt::write(home, &result.name, &receipt)?;

    for dependency in &deps::check_binary_deps(&result.definition) {
        eprintln!("warning: missing binary dependency: {}", dependency.name);
        if let Some(hint) = &dependency.hint {
            eprintln!("  hint: {}", hint);
        }
    }

    let status = if receipt.active {
        "active"
    } else {
        "inactive (missing dependencies)"
    };
    eprintln!("installed {} v{} ({})", result.name, version, status);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util;
    use std::fs;
    fn write_package(path: &Path, yaml: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, yaml).unwrap();
    }

    #[test]
    fn installs_local_dependency_chain() {
        let home = test_util::temp_home("install-deps");
        let temp = tempfile::tempdir().unwrap();
        let dep_path = temp.path().join("dep.yaml");
        let main_path = temp.path().join("main.yaml");

        write_package(
            &dep_path,
            "name: dep\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: dep\n",
        );
        write_package(
            &main_path,
            &format!(
                "name: main\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: main\ndepends:\n  packages:\n    - name: dep\n      source: \"{}\"\n",
                dep_path.display().to_string().replace('\\', "/")
            ),
        );

        run(
            home.path(),
            &main_path.display().to_string(),
            None,
            false,
            false,
            false,
        )
        .unwrap();

        assert!(receipt::read(home.path(), "dep").unwrap().is_some());
        assert!(receipt::read(home.path(), "main").unwrap().is_some());
    }

    #[test]
    fn detects_dependency_cycle() {
        let home = test_util::temp_home("install-cycle");
        let temp = tempfile::tempdir().unwrap();
        let a_path = temp.path().join("a.yaml");
        let b_path = temp.path().join("b.yaml");
        let a_src = a_path.display().to_string().replace('\\', "/");
        let b_src = b_path.display().to_string().replace('\\', "/");

        write_package(
            &a_path,
            &format!(
                "name: a\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: a\ndepends:\n  packages:\n    - name: b\n      source: \"{}\"\n",
                b_src
            ),
        );
        write_package(
            &b_path,
            &format!(
                "name: b\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: b\ndepends:\n  packages:\n    - name: a\n      source: \"{}\"\n",
                a_src
            ),
        );

        let err = run(
            home.path(),
            &a_path.display().to_string(),
            None,
            false,
            false,
            false,
        )
        .unwrap_err();
        assert!(err.to_string().contains("dependency cycle detected"));
    }

    #[test]
    fn version_matches_semver_requirements() {
        assert!(version_matches("1.2.3", "^1.0"));
        assert!(!version_matches("2.0.0", "^1.0"));
        assert!(version_matches("1.2.3", "1.2.3"));
    }

    #[test]
    fn resolve_dep_source_inherits_parent_registry() {
        let dep = PackageDep {
            name: "cat".into(),
            version: Some("^0.2".into()),
            source: None,
        };
        let source = resolve_dep_source(
            &dep,
            &SourceRecord::Registry {
                registry: "default".into(),
                package: "wc".into(),
                version: "0.2.0".into(),
            },
        )
        .unwrap();

        match source {
            source::Source::Registry {
                name,
                version,
                registry,
            } => {
                assert_eq!(name, "cat");
                assert_eq!(version.as_deref(), Some("^0.2"));
                assert_eq!(registry.as_deref(), Some("default"));
            }
            _ => panic!("expected registry source"),
        }
    }
}
