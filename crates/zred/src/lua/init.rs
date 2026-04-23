use mlua::{Lua, Result};
use std::path::{Path, PathBuf};

pub fn load_user_init(lua: &Lua) -> Result<()> {
    for path in init_file_candidates(actual_zr_home(), dirs::config_dir()) {
        if path.is_file() {
            let source = std::fs::read_to_string(&path)?;
            lua.load(&source).set_name(path.to_string_lossy().as_ref()).exec()?;
            break;
        }
    }
    Ok(())
}

fn actual_zr_home() -> Option<PathBuf> {
    zacor_host::paths::zr_home().ok()
}

fn init_file_candidates(zr_home: Option<PathBuf>, config_dir: Option<PathBuf>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(home) = zr_home {
        candidates.push(home.join("zred").join("init.lua"));
    }
    if let Some(config_dir) = config_dir {
        let path = config_dir.join("zred").join("init.lua");
        if !contains_path(&candidates, &path) {
            candidates.push(path);
        }
    }
    candidates
}

fn contains_path(paths: &[PathBuf], candidate: &Path) -> bool {
    paths.iter().any(|path| path == candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_zr_home_init_before_config_dir() {
        let candidates = init_file_candidates(
            Some(PathBuf::from("/tmp/.zr")),
            Some(PathBuf::from("/tmp/.config")),
        );
        assert_eq!(candidates[0], PathBuf::from("/tmp/.zr/zred/init.lua"));
        assert_eq!(candidates[1], PathBuf::from("/tmp/.config/zred/init.lua"));
    }

    #[test]
    fn deduplicates_identical_candidates() {
        let path = PathBuf::from("/tmp/zred");
        let candidates = init_file_candidates(Some(path.clone()), Some(path));
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0], PathBuf::from("/tmp/zred/zred/init.lua"));
    }
}
