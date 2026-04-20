use md5::Md5;
use serde::Serialize;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use std::io::{BufRead, Read};
use std::path::PathBuf;

zacor_package::include_args!();

#[derive(Serialize)]
pub struct HashRecord {
    pub hash: String,
    pub algorithm: String,
    pub file: String,
}

pub fn compute_hash(data: &[u8], algorithm: &str) -> Result<String, String> {
    match algorithm {
        "md5" => Ok(format!("{:x}", Md5::digest(data))),
        "sha1" => Ok(format!("{:x}", Sha1::digest(data))),
        "sha256" => Ok(format!("{:x}", Sha256::digest(data))),
        "sha512" => Ok(format!("{:x}", Sha512::digest(data))),
        _ => Err(format!(
            "hash: unknown algorithm '{}'. Valid: md5, sha1, sha256, sha512",
            algorithm
        )),
    }
}

pub fn hash(
    file: Option<PathBuf>,
    algorithm: &str,
    input: Box<dyn BufRead>,
) -> Result<Vec<HashRecord>, String> {
    match file {
        Some(file_path) => {
            let data = zacor_package::io::fs::read(&file_path)
                .map_err(|e| format!("hash: {}: {e}", file_path.display()))?;
            let hex = compute_hash(&data, algorithm)?;
            Ok(vec![HashRecord {
                hash: hex,
                algorithm: algorithm.to_string(),
                file: file_path.display().to_string(),
            }])
        }
        None => {
            let mut data = Vec::new();
            let mut reader = input;
            reader
                .read_to_end(&mut data)
                .map_err(|e| format!("hash: read stdin: {e}"))?;
            let hex = compute_hash(&data, algorithm)?;
            Ok(vec![HashRecord {
                hash: hex,
                algorithm: algorithm.to_string(),
                file: "stdin".to_string(),
            }])
        }
    }
}
