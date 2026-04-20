use crate::config::{self, RegistryConfig};
use crate::error::*;
use crate::paths;
use crate::registry;
use std::fs;
use std::path::Path;

pub fn add(home: &Path, url: &str, name: Option<&str>) -> Result<()> {
    let registry_name = match name {
        Some(n) => n.to_string(),
        None => derive_name_from_url(url),
    };

    let mut config = config::read_global(home)?;

    if config.registries.iter().any(|r| r.name == registry_name) {
        bail!("registry '{}' already exists", registry_name);
    }

    config.registries.push(RegistryConfig {
        name: registry_name.clone(),
        url: url.to_string(),
    });

    config::write_global(home, &config)?;
    eprintln!("added registry '{}' ({})", registry_name, url);
    Ok(())
}

pub fn remove(home: &Path, name: &str) -> Result<()> {
    let mut config = config::read_global(home)?;

    let before = config.registries.len();
    config.registries.retain(|r| r.name != name);
    if config.registries.len() == before {
        bail!("registry '{}' not found", name);
    }

    config::write_global(home, &config)?;

    // Clean up cached clone
    let cache_dir = paths::registries_dir(home).join(name);
    if cache_dir.exists() {
        let _ = fs::remove_dir_all(&cache_dir);
    }

    eprintln!("removed registry '{}'", name);
    Ok(())
}

pub fn list(home: &Path) -> Result<()> {
    let config = config::read_global(home)?;

    if config.registries.is_empty() {
        println!("no registries configured");
        println!("hint: add a registry with 'zacor registry add <url>'");
        return Ok(());
    }

    for reg in &config.registries {
        let sync_info = match registry::last_sync_time(home, &reg.name) {
            Some(t) => {
                let elapsed = t.elapsed().unwrap_or_default();
                let secs = elapsed.as_secs();
                if secs < 60 {
                    "just now".to_string()
                } else if secs < 3600 {
                    format!("{}m ago", secs / 60)
                } else if secs < 86400 {
                    format!("{}h ago", secs / 3600)
                } else {
                    format!("{}d ago", secs / 86400)
                }
            }
            None => "never".to_string(),
        };
        println!("{} — {} (synced: {})", reg.name, reg.url, sync_info);
    }

    Ok(())
}

pub fn sync(home: &Path, name: Option<&str>) -> Result<()> {
    let config = config::read_global(home)?;

    if config.registries.is_empty() {
        bail!("no registries configured");
    }

    let to_sync: Vec<&RegistryConfig> = if let Some(n) = name {
        let found: Vec<_> = config.registries.iter().filter(|r| r.name == n).collect();
        if found.is_empty() {
            bail!("registry '{}' not found", n);
        }
        found
    } else {
        config.registries.iter().collect()
    };

    for reg in &to_sync {
        if let Err(e) = registry::sync_registry(home, reg) {
            eprintln!("error syncing '{}': {}", reg.name, e);
        } else {
            eprintln!("synced registry '{}'", reg.name);
        }
    }

    Ok(())
}

fn derive_name_from_url(url: &str) -> String {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("registry")
        .trim_end_matches(".git")
        .to_string()
}
