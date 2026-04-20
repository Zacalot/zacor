use crate::error::*;
use crate::receipt;
use std::path::Path;

pub fn run(home: &Path, name: &str, version: &str) -> Result<()> {
    let mut r = receipt::require(home, name)?;

    r.require_version(name, version)?;

    r.current = version.to_string();
    receipt::write(home, name, &r)?;
    eprintln!("switched {} to v{}", name, version);
    Ok(())
}
