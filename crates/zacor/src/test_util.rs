use std::path::PathBuf;
use tempfile::TempDir;

pub fn temp_home(prefix: &str) -> TempDir {
    let dir = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(std::env::temp_dir())
        .unwrap();
    crate::paths::ensure_dirs(dir.path()).unwrap();
    dir
}

pub fn create_dummy_binary(dir: &std::path::Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"#!/bin/sh\necho hello").unwrap();
    path
}
