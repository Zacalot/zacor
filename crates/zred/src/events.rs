use crossterm::event::{self, Event, KeyEvent};
use std::time::Duration;

pub enum AppEvent {
    Key(KeyEvent),
    Tick,
}

pub fn next(timeout: Duration) -> std::io::Result<AppEvent> {
    if event::poll(timeout)? {
        loop {
            match event::read()? {
                Event::Key(key) => return Ok(AppEvent::Key(key)),
                Event::Resize(_, _) => return Ok(AppEvent::Tick),
                _ => {}
            }
        }
    }

    Ok(AppEvent::Tick)
}
