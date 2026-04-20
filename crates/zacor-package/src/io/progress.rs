//! Progress reporting via the protocol.
//!
//! Fire-and-forget — the module does not wait for a response.
//! Silently no-ops in Local (library) mode.

use super::ExecMode;

/// Report execution progress. `fraction` should be between 0.0 and 1.0.
/// Fire-and-forget: does not block, errors are silently ignored.
pub fn report(fraction: f64) {
    if super::mode() == ExecMode::Local {
        return; // No protocol, silently ignore
    }
    crate::runtime::send_progress(fraction);
}
