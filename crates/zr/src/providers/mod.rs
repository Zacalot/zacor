use std::sync::Arc;
use zacor_host::capability::CapabilityRegistry;

mod clipboard;
mod fs;
mod http_forwarder;
mod prompt;
mod subprocess;

pub use clipboard::ClipboardProvider;
pub use fs::FsProvider;
pub use http_forwarder::HttpForwarder;
pub use prompt::PromptProvider;
pub use subprocess::SubprocessProvider;

pub fn build_default_registry() -> CapabilityRegistry {
    let mut registry = CapabilityRegistry::new();
    registry.register(Arc::new(FsProvider)).expect("unique fs provider");
    registry
        .register(Arc::new(ClipboardProvider))
        .expect("unique clipboard provider");
    registry
        .register(Arc::new(PromptProvider))
        .expect("unique prompt provider");
    registry
        .register(Arc::new(HttpForwarder))
        .expect("unique http provider");
    registry
        .register(Arc::new(SubprocessProvider))
        .expect("unique subprocess provider");
    registry
}

pub(crate) fn invalid_input(message: impl Into<String>) -> zacor_host::protocol::CapabilityError {
    zacor_host::protocol::CapabilityError {
        kind: "invalid_input".into(),
        message: message.into(),
    }
}

pub(crate) fn resolve_path(path_str: &str) -> String {
    let cwd = std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let resolved = zacor_host::protocol::resolve_path(path_str, &cwd);
    resolved.replace('/', std::path::MAIN_SEPARATOR_STR)
}
