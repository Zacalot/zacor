use crate::frontends::tui::events::AppEvent;
use crate::frontends::tui::terminal::TerminalGuard;
use crate::session::SessionFrontendEffect;
use crate::shell::{App, AppShell};
use anyhow::Result;
use std::time::Duration;

pub mod events;
pub mod input;
pub mod render;
pub mod terminal;

const TICK_RATE: Duration = Duration::from_millis(100);

pub fn run() -> Result<()> {
    let mut terminal = TerminalGuard::enter()?;
    let mut app = App::new()?;
    run_loop(&mut app, &mut terminal)
}

fn run_loop(app: &mut impl AppShell, terminal: &mut TerminalGuard) -> Result<()> {
    loop {
        if app.should_quit() {
            break;
        }
        drain_frontend_effects(app);
        terminal.draw(|frame| {
            let view = app.view();
            render::render(frame, &view)
        })?;
        match events::next(TICK_RATE)? {
            AppEvent::Key(key) => {
                app.handle_input(input::app_input_event_from_tui(key));
                drain_frontend_effects(app);
            }
            AppEvent::Tick => {}
        }
    }

    Ok(())
}

fn drain_frontend_effects(app: &mut impl AppShell) {
    let effects = app.drain_frontend_effects();
    if effects.is_empty() {
        return;
    }

    for effect in effects {
        match effect {
            SessionFrontendEffect::NewWindow => {
                app.set_status("Current frontend does not support opening new windows");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tui_drains_unsupported_frontend_effects_into_status() {
        let mut app = App::new().expect("app should initialize");

        app.run_command("window.new");
        drain_frontend_effects(&mut app);

        assert!(app.drain_frontend_effects().is_empty());
        assert_eq!(
            app.view().minibuffer_text,
            "Current frontend does not support opening new windows"
        );
    }
}
