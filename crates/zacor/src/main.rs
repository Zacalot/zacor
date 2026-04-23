#![warn(clippy::all)]

pub use zacor_host::{
    config, error, host as wasm_runtime, manifest as wasm_manifest, package_definition, paths,
    platform, receipt,
};

#[cfg(test)]
pub use zacor_host::test_util;

mod cli;
mod deps;
#[path = "dispatch/clap_builder.rs"]
mod dispatch_clap_builder;
mod execute;
mod registry;
mod serve;
mod source;
mod store;

pub(crate) fn resolve_zr_binary() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("ZR_BIN") {
        let path = std::path::PathBuf::from(path);
        if path.exists() {
            return path;
        }
    }

    if let Ok(path) = std::env::var("CARGO_BIN_EXE_zr") {
        let path = std::path::PathBuf::from(path);
        if path.exists() {
            return path;
        }
    }

    let current = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("zacor"));
    let search_roots = [
        current.parent().map(std::path::Path::to_path_buf),
        current
            .parent()
            .and_then(std::path::Path::parent)
            .map(std::path::Path::to_path_buf),
    ];

    for root in search_roots.into_iter().flatten() {
        let direct = root.join(format!("zr{}", std::env::consts::EXE_SUFFIX));
        if direct.exists() {
            return direct;
        }

        let deps = root.join("deps");
        if let Some(path) = find_zr_artifact(&deps) {
            return path;
        }
    }

    std::path::PathBuf::from(format!("zr{}", std::env::consts::EXE_SUFFIX))
}

pub(crate) fn resolve_zr_daemon_binary() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("ZR_DAEMON_BIN") {
        let path = std::path::PathBuf::from(path);
        if path.exists() {
            return path;
        }
    }

    if let Ok(path) = std::env::var("CARGO_BIN_EXE_zr-daemon") {
        let path = std::path::PathBuf::from(path);
        if path.exists() {
            return path;
        }
    }

    let current = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("zacor"));
    let search_roots = [
        current.parent().map(std::path::Path::to_path_buf),
        current
            .parent()
            .and_then(std::path::Path::parent)
            .map(std::path::Path::to_path_buf),
    ];

    for root in search_roots.into_iter().flatten() {
        let direct = root.join(format!("zr-daemon{}", std::env::consts::EXE_SUFFIX));
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
                        .is_some_and(|stem| stem == "zr-daemon" || stem.starts_with("zr-daemon-"))
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
            .join(format!("zr-daemon{}", std::env::consts::EXE_SUFFIX));
        if target_path.exists() {
            return target_path;
        }

        let status = std::process::Command::new("cargo")
            .args(["build", "-p", "zr-daemon", "--bin", "zr-daemon"])
            .env("CARGO_TARGET_DIR", &bootstrap_target_dir)
            .current_dir(&workspace_root)
            .status();
        if matches!(status, Ok(status) if status.success()) && target_path.exists() {
            return target_path;
        }
    }

    std::path::PathBuf::from(format!("zr-daemon{}", std::env::consts::EXE_SUFFIX))
}

fn find_zr_artifact(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut candidates = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some(std::env::consts::EXE_EXTENSION))
        .filter(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem == "zr" || stem.starts_with("zr-"))
        })
        .collect::<Vec<_>>();

    candidates.sort();
    candidates.into_iter().next()
}

fn main() {
    std::process::exit(cli::run_zacor());
}
