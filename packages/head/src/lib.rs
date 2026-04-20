use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use zacor_package::io::fs as zr_fs;

zacor_package::include_args!();

#[derive(Serialize)]
pub struct HeadRecord {
    pub line: usize,
    pub content: String,
}

pub fn head(
    file: Option<PathBuf>,
    max_lines: usize,
    input: Box<dyn BufRead>,
) -> Result<impl Iterator<Item = HeadRecord>, String> {
    let reader: Box<dyn BufRead> = match file {
        Some(f) => Box::new(BufReader::new(
            zr_fs::open_stream(&f).map_err(|e| format!("head: {}: {}", f.display(), e))?,
        )),
        None => input,
    };

    Ok(reader.lines().take(max_lines).enumerate().map(|(i, line)| {
        let content = line.unwrap_or_else(|e| {
            eprintln!("head: read error: {}", e);
            String::new()
        });
        HeadRecord {
            line: i + 1,
            content,
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zacor_package::FromArgs;
    use std::collections::BTreeMap;
    use serde_json::json;

    #[test]
    fn from_args_optional_missing() {
        let map: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        let args = args::DefaultArgs::from_args(&map).unwrap();
        assert!(args.file.is_none());
    }

    #[test]
    fn from_args_optional_pathbuf() {
        let map: BTreeMap<String, _> = [("file".into(), json!("/tmp/data.txt"))].into();
        let args = args::DefaultArgs::from_args(&map).unwrap();
        assert_eq!(args.file, Some(PathBuf::from("/tmp/data.txt")));
    }

    #[test]
    fn from_args_number_default() {
        let map: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        let args = args::DefaultArgs::from_args(&map).unwrap();
        assert_eq!(args.lines, 10);
    }

    #[test]
    fn from_args_number_explicit() {
        let map: BTreeMap<String, _> = [("lines".into(), json!(5))].into();
        let args = args::DefaultArgs::from_args(&map).unwrap();
        assert_eq!(args.lines, 5);
    }

    #[test]
    fn from_args_number_from_string() {
        let map: BTreeMap<String, _> = [("lines".into(), json!("20"))].into();
        let args = args::DefaultArgs::from_args(&map).unwrap();
        assert_eq!(args.lines, 20);
    }
}
