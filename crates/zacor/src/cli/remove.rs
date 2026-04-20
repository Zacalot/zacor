use crate::deps;
use crate::error::*;
use crate::receipt::{self};
use crate::store;
use std::path::Path;

pub fn run(home: &Path, name: &str, force: bool) -> Result<()> {
    // Parse name@version syntax
    let (package_name, version) = if let Some(at) = name.rfind('@') {
        (&name[..at], Some(&name[at + 1..]))
    } else {
        (name, None)
    };

    let mut receipt = receipt::require(home, package_name)?;

    // Check reverse dependencies (unless --force)
    if !force {
        let dependents = deps::find_reverse_deps(home, package_name)?;
        if !dependents.is_empty() {
            bail!(
                "cannot remove '{}': depended on by: {}\nhint: use --force to remove anyway (dependents will be disabled)",
                package_name,
                dependents.join(", ")
            );
        }
    }

    if let Some(ver) = version {
        // Remove specific version
        receipt.require_version(package_name, ver)?;

        store::remove_version(home, package_name, ver)?;
        let should_delete = receipt.remove_version(ver);

        if should_delete {
            receipt::delete(home, package_name)?;
            eprintln!("removed {} (last version)", package_name);
        } else {
            receipt::write(home, package_name, &receipt)?;
            eprintln!(
                "removed {} v{} (current: {})",
                package_name, ver, receipt.current
            );
        }
    } else {
        // Remove entire package
        store::remove(home, package_name)?;
        receipt::delete(home, package_name)?;

        if force {
            disable_dependents(home, package_name)?;
        }

        eprintln!("removed {}", package_name);
    }

    Ok(())
}

fn disable_dependents(home: &Path, package_name: &str) -> Result<()> {
    let dependents = deps::find_reverse_deps(home, package_name)?;
    for dep_name in &dependents {
        if let Ok(Some(mut r)) = receipt::read(home, dep_name) {
            r.active = false;
            receipt::write(home, dep_name, &r)?;
            eprintln!("disabled dependent: {}", dep_name);
        }
    }
    Ok(())
}
