use serde::Serialize;
use std::io::{BufRead, Read};
use std::path::PathBuf;
use zacor_package::io::fs as zr_fs;

zacor_package::include_args!();

#[derive(Serialize)]
pub struct WcRecord {
    pub file: String,
    pub lines: usize,
    pub words: usize,
    pub bytes: usize,
}

pub fn wc(file: Option<PathBuf>, input: Box<dyn BufRead>) -> Result<WcRecord, String> {
    let (name, content) = match file {
        Some(f) => {
            let content =
                zr_fs::read_string(&f).map_err(|e| format!("wc: {}: {}", f.display(), e))?;
            (f.display().to_string(), content)
        }
        None => {
            let mut buf = String::new();
            input
                .take(u64::MAX)
                .read_to_string(&mut buf)
                .map_err(|e| format!("wc: stdin: {}", e))?;
            ("stdin".to_string(), buf)
        }
    };

    Ok(WcRecord {
        file: name,
        lines: content.lines().count(),
        words: content.split_whitespace().count(),
        bytes: content.len(),
    })
}
