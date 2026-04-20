use crate::config;
use crate::error::*;
use crate::platform;
use crate::receipt;
use std::path::Path;

pub fn run(
    home: &Path,
    key_value: Option<&str>,
    value: Option<&str>,
    unset: bool,
    global: bool,
    list: bool,
    edit: bool,
) -> Result<()> {
    if edit {
        let config_path = crate::paths::config_path(home);
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
        std::process::Command::new(&editor)
            .arg(&config_path)
            .status()
            .with_context(|| format!("failed to open editor '{}'", editor))?;
        return Ok(());
    }

    let key_value =
        key_value.ok_or_else(|| anyhow!("usage: zacor config <package>.<key> [value]"))?;

    // Parse package.key
    let (package_name, key) = key_value
        .split_once('.')
        .ok_or_else(|| anyhow!("expected <package>.<key> format, got '{}'", key_value))?;

    if list {
        return list_config(home, package_name);
    }

    platform::validate_config_key(key)?;

    if unset {
        if global {
            let mut gc = config::read_global(home)?;
            if let Some(section) = gc.packages.get_mut(package_name) {
                section.remove(key);
            }
            config::write_global(home, &gc)?;
            eprintln!("unset {}.{} (global)", package_name, key);
        } else {
            let mut r = receipt::require(home, package_name)?;
            r.unset_config(key);
            receipt::write(home, package_name, &r)?;
            eprintln!("unset {}.{}", package_name, key);
        }
        return Ok(());
    }

    if let Some(val) = value {
        // Set
        if global {
            let mut gc = config::read_global(home)?;
            gc.packages
                .entry(package_name.to_string())
                .or_default()
                .insert(key.to_string(), val.to_string());
            config::write_global(home, &gc)?;
            eprintln!("set {}.{} = {} (global)", package_name, key, val);
        } else {
            let mut r = receipt::require(home, package_name)?;
            r.set_config(key.to_string(), val.to_string());
            receipt::write(home, package_name, &r)?;
            eprintln!("set {}.{} = {}", package_name, key, val);
        }
    } else {
        // Get — print resolved value
        let r = receipt::require(home, package_name)?;
        let gc = config::read_global(home)?;
        let pkg_config = crate::wasm_manifest::load_from_store(home, package_name, &r.current)
            .map(|def| def.config)
            .unwrap_or_default();
        let flags = std::collections::BTreeMap::new();
        if let Some(val) = config::resolve(key, package_name, &flags, &r, &gc, &pkg_config, None) {
            println!("{}", val);
        }
    }

    Ok(())
}

fn list_config(home: &Path, package_name: &str) -> Result<()> {
    let r = receipt::require(home, package_name)?;

    if r.config.is_empty() {
        println!("no config set for {}", package_name);
    } else {
        for (k, v) in &r.config {
            println!("{}.{} = {}", package_name, k, v);
        }
    }

    Ok(())
}
