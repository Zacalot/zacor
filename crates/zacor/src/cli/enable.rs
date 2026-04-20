use crate::deps;
use crate::error::*;
use crate::receipt;
use std::path::Path;

pub fn run(home: &Path, name: &str) -> Result<()> {
    let mut r = receipt::require(home, name)?;

    if r.active {
        eprintln!("{} is already active", name);
        return Ok(());
    }

    // Check dependencies before enabling
    if let Ok(def) = crate::wasm_manifest::load_from_store(home, name, &r.current) {
        let unsatisfied = deps::check_package_deps(home, &def)?;
        if let Some(u) = unsatisfied.first() {
            let status = if u.installed {
                "not active"
            } else {
                "not installed"
            };
            bail!(
                "cannot enable '{}': dependency '{}' is {}",
                name,
                u.name,
                status
            );
        }
    }

    r.active = true;
    receipt::write(home, name, &r)?;
    eprintln!("enabled {}", name);
    Ok(())
}
