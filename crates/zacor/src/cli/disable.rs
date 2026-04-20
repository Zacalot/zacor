use crate::error::*;
use crate::receipt;
use std::path::Path;

pub fn run(home: &Path, name: &str) -> Result<()> {
    let mut r = receipt::require(home, name)?;

    if !r.active {
        eprintln!("{} is already inactive", name);
        return Ok(());
    }

    r.active = false;
    receipt::write(home, name, &r)?;
    eprintln!("disabled {}", name);
    Ok(())
}
