use crate::frontends::native;
use crate::frontends::tui;
use anyhow::Result;

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FrontendKind {
    Native,
    Tui,
}

fn frontend_for_args(args: impl IntoIterator<Item = impl AsRef<str>>) -> FrontendKind {
    if args.into_iter().any(|arg| arg.as_ref() == "--tui") {
        FrontendKind::Tui
    } else {
        FrontendKind::Native
    }
}

pub fn run() -> Result<()> {
    match frontend_for_args(std::env::args()) {
        FrontendKind::Native => native::run(),
        FrontendKind::Tui => tui::run(),
    }
}

#[cfg(test)]
mod frontend_tests {
    use super::{frontend_for_args, FrontendKind};

    #[test]
    fn defaults_to_native_frontend() {
        assert_eq!(frontend_for_args(["zred"]), FrontendKind::Native);
    }

    #[test]
    fn keeps_native_when_native_flag_is_present() {
        assert_eq!(frontend_for_args(["zred", "--native"]), FrontendKind::Native);
    }

    #[test]
    fn allows_explicit_tui_fallback() {
        assert_eq!(frontend_for_args(["zred", "--tui"]), FrontendKind::Tui);
    }
}
