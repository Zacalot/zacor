pub type Result<T> = anyhow::Result<T>;
pub use anyhow::{Context, anyhow, bail};

#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("package not found: {0}")]
    PackageNotFound(String),

    #[error("package '{0}' is disabled - run `zacor enable {0}`")]
    Disabled(String),

    #[error("daemon unavailable")]
    DaemonUnavailable,

    #[error("daemon refused: {0:?}")]
    DaemonRefused(zacor_protocol::DaemonRefusal),

    #[error("artifact missing: {0}")]
    ArtifactMissing(std::path::PathBuf),

    #[error("wasm runtime error")]
    WasmRuntime(#[source] anyhow::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("transport: {0}")]
    Transport(#[source] anyhow::Error),

    #[error("protocol: {0}")]
    Protocol(String),

    #[error("call depth exceeded")]
    DepthExceeded,

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
