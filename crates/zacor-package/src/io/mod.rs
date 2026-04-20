//! IO abstraction layer for transparent local/remote/protocol operation.
//!
//! The execution mode determines how capabilities are routed:
//! - `Local`: All operations use local APIs directly. No protocol.
//! - `ProtocolLocal`: Protocol active, but fs/clipboard use local fast path.
//! - `ProtocolRemote`: Protocol active, all capabilities use protocol dispatch.

pub mod fs;
pub mod http;
pub mod progress;
pub mod prompt;

use std::sync::OnceLock;

/// Execution mode that determines capability routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecMode {
    /// Direct local execution — no protocol. fs/clipboard: local. prompt/progress: unavailable.
    Local,
    /// Protocol with local filesystem — fs/clipboard use local fast path,
    /// prompt/progress use protocol. Set by `zr::run()` for CLI execution.
    ProtocolLocal,
    /// Protocol without local filesystem — all capabilities route through protocol.
    /// Used when the module runs remotely (HTTP transport).
    ProtocolRemote,
}

static MODE: OnceLock<ExecMode> = OnceLock::new();

/// Get the current execution mode. Defaults to `Local`.
pub fn mode() -> ExecMode {
    *MODE.get_or_init(|| ExecMode::Local)
}

/// Set the execution mode. Called during runtime initialization.
pub(crate) fn set_mode(m: ExecMode) {
    let _ = MODE.set(m);
}
