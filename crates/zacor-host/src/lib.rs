#![warn(clippy::all)]

pub mod capability;
#[path = "../../zacor/src/config.rs"]
pub mod config;
#[path = "../../zacor/src/error.rs"]
pub mod error;
#[path = "../../zacor/src/wasm_runtime.rs"]
pub mod host;
#[path = "../../zacor/src/wasm_manifest.rs"]
pub mod manifest;
#[path = "../../zacor/src/package_definition.rs"]
pub mod package_definition;
#[path = "../../zacor/src/paths.rs"]
pub mod paths;
#[path = "../../zacor/src/platform.rs"]
pub mod platform;
#[path = "../../zacor/src/receipt.rs"]
pub mod receipt;
pub mod router;
pub mod session;

#[cfg(any(test, feature = "testing"))]
#[path = "../../zacor/src/test_util.rs"]
pub mod test_util;

pub use zacor_protocol as protocol;
