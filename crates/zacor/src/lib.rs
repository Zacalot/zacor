#![warn(clippy::all)]

mod capability_provider;
mod cli;
mod config;
pub(crate) mod daemon;
pub(crate) mod daemon_client;
mod deps;
mod dispatch;
mod error;
mod execute;
#[cfg(windows)]
mod job_object;
mod package_definition;
mod paths;
mod platform;
mod receipt;
mod registry;
mod render;
mod serve;
mod source;
mod store;
#[cfg(test)]
mod test_util;
mod wasm_manifest;
mod wasm_runtime;

use std::env;

pub fn run(binary_name: Option<&str>) -> i32 {
    let binary_name = binary_name
        .map(str::to_owned)
        .or_else(|| {
            env::args().next().and_then(|arg| {
                std::path::Path::new(&arg)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
            })
        })
        .unwrap_or_else(|| "zacor".to_string());

    if binary_name.eq_ignore_ascii_case("zr") {
        cli::run_zr()
    } else {
        cli::run_zacor()
    }
}
