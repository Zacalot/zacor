#![warn(clippy::all)]

pub use zacor_host::{
    config, error, host as wasm_runtime, manifest as wasm_manifest, package_definition, paths,
    platform, receipt,
};

#[cfg(test)]
pub use zacor_host::test_util;

#[path = "../../zr/src/daemon/mod.rs"]
mod daemon;
#[cfg(windows)]
#[path = "../../zacor/src/job_object.rs"]
mod job_object;

mod provider_support {
    pub(crate) fn invalid_input(message: impl Into<String>) -> zacor_host::protocol::CapabilityError {
        zacor_host::protocol::CapabilityError {
            kind: "invalid_input".into(),
            message: message.into(),
        }
    }
}

fn main() {
    let home = match paths::zr_home() {
        Ok(home) => home,
        Err(error) => {
            eprintln!("error: {:#}", error);
            std::process::exit(1);
        }
    };

    if let Err(error) = paths::ensure_dirs(&home) {
        eprintln!("error: {:#}", error);
        std::process::exit(1);
    }

    let server = daemon::DaemonServer::new(home);
    match server.run() {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            eprintln!("error: {:#}", error);
            std::process::exit(1);
        }
    }
}
