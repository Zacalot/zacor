use crate::frontends::tui;
use crate::frontends::tui::events::AppEvent;
use crate::frontends::tui::terminal::TerminalGuard;
use crate::runtime::AppRuntime;
use crate::session::Session;
use anyhow::Result;
use crossterm::event::KeyEvent;
use std::cell::Ref;
use std::time::Duration;

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

const TICK_RATE: Duration = Duration::from_millis(100);

pub fn run() -> Result<()> {
    let mut terminal = TerminalGuard::enter()?;
    let mut app = App::new()?;

    loop {
        if app.state().should_quit() {
            break;
        }
        terminal.draw(|frame| {
            let state = app.state();
            tui::render::render(frame, &state)
        })?;
        match tui::events::next(TICK_RATE)? {
            AppEvent::Key(key) => app.handle_key(key),
            AppEvent::Tick => {}
        }
    }

    Ok(())
}

pub struct App {
    runtime: AppRuntime,
}

impl App {
    pub fn new() -> Result<Self> {
        let state = Session::shared();
        Ok(Self {
            runtime: AppRuntime::new(state)?,
        })
    }

    pub fn state(&self) -> Ref<'_, Session> {
        self.runtime.state()
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        self.runtime.handle_key(key);
    }
}
