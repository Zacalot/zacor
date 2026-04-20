use crate::deps;
use crate::error::*;
use crate::platform;
use crate::receipt::{self, Receipt};
use crate::source;
use crate::store;
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
    let result = source::acquire(&parsed, name_override, home, from_source)?;

    let version = platform::normalize_version(&result.definition.version);

    // Check if already installed
    let existing = receipt::read(home, &result.name)?;
    if let Some(ref r) = existing
        && r.versions.contains_key(&version)
        && !force
    {
        eprintln!("{} v{} is already installed", result.name, version);
        return Ok(());
    }

    // Store package (content directory → store)
    store::put(home, &result.name, &version, result.content_dir.path())
        .context("failed to store package")?;

    // Check dependencies
    let unsatisfied = deps::check_package_deps(home, &result.definition)?;
    let deps_satisfied = unsatisfied.is_empty();
    for u in &unsatisfied {
        eprintln!("warning: missing dependency: {}", u.name);
    }

    // Create or update receipt
    let mut receipt = match existing {
        Some(mut r) => {
            r.add_version(version.clone(), result.source_info);
            r
        }
        None => Receipt::new(version.clone(), result.source_info),
    };
    if !deps_satisfied || inactive {
        receipt.active = false;
    }

    receipt::write(home, &result.name, &receipt)?;

    // Check binary deps (warning only)
    for u in &deps::check_binary_deps(&result.definition) {
        eprintln!("warning: missing binary dependency: {}", u.name);
        if let Some(ref hint) = u.hint {
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
