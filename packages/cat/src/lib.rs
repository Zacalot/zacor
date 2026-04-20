use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use zacor_package::io::fs as zr_fs;

zacor_package::include_args!();

#[derive(Serialize)]
pub struct CatRecord {
    pub line: usize,
    pub content: String,
}

pub fn cat(
    file: Option<PathBuf>,
    lines: Option<i64>,
    tail: Option<i64>,
    input: Box<dyn BufRead>,
) -> Result<Vec<CatRecord>, String> {
    if lines.is_some() && tail.is_some() {
        return Err("cat: --lines and --tail are mutually exclusive".into());
    }

    let reader: Box<dyn BufRead> = match file {
        Some(f) => Box::new(BufReader::new(
            zr_fs::open_stream(&f).map_err(|e| format!("cat: {}: {}", f.display(), e))?,
        )),
        None => input,
    };

    let all_lines: Vec<CatRecord> = reader.lines().enumerate().map(|(i, line)| {
        let content = line.unwrap_or_else(|e| {
            eprintln!("cat: read error: {}", e);
            String::new()
        });
        CatRecord {
            line: i + 1,
            content,
        }
    }).collect();

    if let Some(n) = lines {
        let n = n.max(0) as usize;
        Ok(all_lines.into_iter().take(n).collect())
    } else if let Some(n) = tail {
        let n = n.max(0) as usize;
        let len = all_lines.len();
        let skip = len.saturating_sub(n);
        Ok(all_lines.into_iter().skip(skip).collect())
    } else {
        Ok(all_lines)
    }
}
