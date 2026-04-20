use serde::Serialize;
use std::path::PathBuf;
use zacor_package::io::fs as zr_fs;

zacor_package::include_args!();

#[derive(Serialize)]
pub struct LsRecord {
    pub name: String,
    pub size: u64,
    pub kind: String,
}

pub fn ls(path: PathBuf, all: bool) -> Result<Vec<LsRecord>, String> {
    let mut entries = zr_fs::read_dir(&path)
        .map_err(|e| format!("ls: {}: {}", path.display(), e))?;

    entries.sort_by(|a, b| a.name.cmp(&b.name));

    let records: Vec<LsRecord> = entries
        .into_iter()
        .filter_map(|entry| {
            if !all && entry.name.starts_with('.') {
                return None;
            }
            Some(LsRecord {
                size: entry.size,
                kind: match entry.file_type {
                    zr_fs::FileType::Dir => "dir".to_string(),
                    _ => "file".to_string(),
                },
                name: entry.name,
            })
        })
        .collect();

    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zacor_package::FromArgs;
    use std::collections::BTreeMap;
    use serde_json::json;

    #[test]
    fn from_args_bool_true() {
        let map: BTreeMap<String, _> = [("all".into(), json!(true))].into();
        let args = args::DefaultArgs::from_args(&map).unwrap();
        assert!(args.all);
    }

    #[test]
    fn from_args_bool_from_string() {
        let map: BTreeMap<String, _> = [("all".into(), json!("true"))].into();
        let args = args::DefaultArgs::from_args(&map).unwrap();
        assert!(args.all);
    }

    #[test]
    fn from_args_bool_default_false() {
        let map: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        let args = args::DefaultArgs::from_args(&map).unwrap();
        assert!(!args.all);
    }

    #[test]
    fn from_args_path_with_default() {
        let map: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        let args = args::DefaultArgs::from_args(&map).unwrap();
        assert_eq!(args.path, PathBuf::from("."));
    }

    #[test]
    fn from_args_path_explicit() {
        let map: BTreeMap<String, _> = [("path".into(), json!("/tmp"))].into();
        let args = args::DefaultArgs::from_args(&map).unwrap();
        assert_eq!(args.path, PathBuf::from("/tmp"));
    }
}
