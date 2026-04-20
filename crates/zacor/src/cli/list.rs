use crate::error::*;
use crate::receipt;
use std::path::Path;

pub fn run(home: &Path) -> Result<()> {
    let packages = receipt::list_all(home)?;

    if packages.is_empty() {
        println!("no packages installed");
        return Ok(());
    }

    let mut name_w = 4;
    let mut ver_w = 7;
    for (name, r) in &packages {
        name_w = name_w.max(name.len());
        ver_w = ver_w.max(r.current.len());
    }

    println!(
        "{:<name_w$}  {:<ver_w$}  {:<8}  DESCRIPTION",
        "NAME",
        "VERSION",
        "STATUS",
        name_w = name_w,
        ver_w = ver_w,
    );

    for (name, r) in &packages {
        let status = if r.active { "active" } else { "inactive" };
        let desc = get_description(home, name, &r.current);
        println!(
            "{:<name_w$}  {:<ver_w$}  {:<8}  {}",
            name,
            r.current,
            status,
            desc,
            name_w = name_w,
            ver_w = ver_w,
        );
    }

    Ok(())
}

fn get_description(home: &Path, name: &str, version: &str) -> String {
    if let Ok(def) = crate::wasm_manifest::load_from_store(home, name, version) {
        def.description.unwrap_or_default()
    } else {
        String::new()
    }
}
