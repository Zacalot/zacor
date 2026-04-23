#![warn(clippy::all)]

pub use zacor_host::{
    config, error, host as wasm_runtime, manifest as wasm_manifest, package_definition, paths,
    platform, receipt,
};

#[cfg(test)]
pub use zacor_host::test_util;

mod daemon_client;
#[path = "../../zacor/src/dispatch/mod.rs"]
mod dispatch;
#[path = "../../zacor/src/execute.rs"]
mod execute;
#[cfg(windows)]
#[path = "../../zacor/src/job_object.rs"]
mod job_object;
mod providers;
#[path = "../../zacor/src/render.rs"]
mod render;
#[path = "../../zacor/src/cli/zr.rs"]
mod cli;

pub(crate) fn resolve_peer_binary(name: &str) -> std::path::PathBuf {
    let env_name = format!("CARGO_BIN_EXE_{name}");
    if let Ok(path) = std::env::var(&env_name) {
        let path = std::path::PathBuf::from(path);
        if path.exists() {
            return path;
        }
    }

    let current = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from(name));
    let search_roots = [
        current.parent().map(std::path::Path::to_path_buf),
        current
            .parent()
            .and_then(std::path::Path::parent)
            .map(std::path::Path::to_path_buf),
    ];

    for root in search_roots.into_iter().flatten() {
        let direct = root.join(format!("{}{}", name, std::env::consts::EXE_SUFFIX));
        if direct.exists() {
            return direct;
        }

        let deps = root.join("deps");
        let mut candidates = std::fs::read_dir(&deps)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension().and_then(|ext| ext.to_str()) == Some(std::env::consts::EXE_EXTENSION)
                    && path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .is_some_and(|stem| stem == name || stem.starts_with(&format!("{name}-")))
            })
            .collect::<Vec<_>>();
        candidates.sort();
        if let Some(path) = candidates.into_iter().next() {
            return path;
        }
    }

    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .map(std::path::Path::to_path_buf);
    if let Some(workspace_root) = workspace_root {
        let bootstrap_target_dir = workspace_root.join("target").join("peer-bin-bootstrap");
        let target_path = bootstrap_target_dir
            .join("debug")
            .join(format!("{}{}", name, std::env::consts::EXE_SUFFIX));
        if target_path.exists() {
            return target_path;
        }

        let status = std::process::Command::new("cargo")
            .arg("build")
            .arg("-p")
            .arg(name)
            .arg("--bin")
            .arg(name)
            .env("CARGO_TARGET_DIR", &bootstrap_target_dir)
            .current_dir(&workspace_root)
            .status();
        if matches!(status, Ok(status) if status.success()) && target_path.exists() {
            return target_path;
        }
    }

    std::path::PathBuf::from(format!("{}{}", name, std::env::consts::EXE_SUFFIX))
}

fn main() {
    std::process::exit(cli::run());
}
