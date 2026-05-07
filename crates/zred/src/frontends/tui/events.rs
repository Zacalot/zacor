use super::input::TuiKeyEvent;
use crossterm::event::{self, Event};
use std::time::Duration;

pub enum AppEvent {
    Key(TuiKeyEvent),
    Tick,
}

pub fn next(timeout: Duration) -> std::io::Result<AppEvent> {
    if event::poll(timeout)? {
        loop {
            match event::read()? {
                Event::Key(key) => return Ok(AppEvent::Key(key.into())),
                Event::Resize(_, _) => return Ok(AppEvent::Tick),
                _ => {}
            }
        }
    }

    Ok(AppEvent::Tick)
}
