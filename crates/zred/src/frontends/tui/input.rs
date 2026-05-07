use crate::kernel::{KeyChord, KeyCodeRepr, KeyModifiersRepr};
use crate::session::AppInputEvent;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TuiKeyEvent {
    code: TuiKeyCode,
    modifiers: TuiKeyModifiers,
}

impl From<KeyEvent> for TuiKeyEvent {
    fn from(value: KeyEvent) -> Self {
        Self {
            code: match value.code {
                KeyCode::Backspace => TuiKeyCode::Backspace,
                KeyCode::Enter => TuiKeyCode::Enter,
                KeyCode::Esc => TuiKeyCode::Esc,
                KeyCode::Char(ch) => TuiKeyCode::Char(ch),
                _ => TuiKeyCode::Unsupported,
            },
            modifiers: TuiKeyModifiers {
                control: value.modifiers.contains(KeyModifiers::CONTROL),
                shift: value.modifiers.contains(KeyModifiers::SHIFT),
                alt: value.modifiers.contains(KeyModifiers::ALT),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TuiKeyCode {
    Backspace,
    Enter,
    Esc,
    Char(char),
    Unsupported,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TuiKeyModifiers {
    control: bool,
    shift: bool,
    alt: bool,
}

pub fn key_chord_from_event(event: TuiKeyEvent) -> Option<KeyChord> {
    match event.code {
        TuiKeyCode::Char(ch) => Some(KeyChord::new(
            KeyCodeRepr::Char(ch),
            printable_modifiers(event.modifiers),
        )),
        TuiKeyCode::Enter => Some(KeyChord::new(
            KeyCodeRepr::Enter,
            non_printable_modifiers(event.modifiers),
        )),
        TuiKeyCode::Esc => Some(KeyChord::new(
            KeyCodeRepr::Esc,
            non_printable_modifiers(event.modifiers),
        )),
        TuiKeyCode::Backspace => Some(KeyChord::new(
            KeyCodeRepr::Backspace,
            non_printable_modifiers(event.modifiers),
        )),
        _ => None,
    }
}

fn printable_modifiers(modifiers: TuiKeyModifiers) -> KeyModifiersRepr {
    KeyModifiersRepr::new(modifiers.control, false, modifiers.alt)
}

fn non_printable_modifiers(modifiers: TuiKeyModifiers) -> KeyModifiersRepr {
    KeyModifiersRepr::new(modifiers.control, modifiers.shift, modifiers.alt)
}

pub fn text_input_from_event(event: TuiKeyEvent) -> Option<char> {
    match event.code {
        TuiKeyCode::Char(ch) if !event.modifiers.control => Some(ch),
        _ => None,
    }
}

pub fn app_input_event_from_tui(event: TuiKeyEvent) -> AppInputEvent {
    AppInputEvent::new(key_chord_from_event(event), text_input_from_event(event))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printable_character_chords_ignore_shift() {
        let chord = key_chord_from_event(TuiKeyEvent {
            code: TuiKeyCode::Char(':'),
            modifiers: TuiKeyModifiers {
                control: false,
                shift: true,
                alt: false,
            },
        });

        assert_eq!(
            chord,
            Some(KeyChord::new(
                KeyCodeRepr::Char(':'),
                KeyModifiersRepr::NONE
            ))
        );
    }

    #[test]
    fn printable_character_chords_keep_control() {
        let chord = key_chord_from_event(TuiKeyEvent {
            code: TuiKeyCode::Char('H'),
            modifiers: TuiKeyModifiers {
                control: true,
                shift: true,
                alt: false,
            },
        });

        assert_eq!(
            chord,
            Some(KeyChord::new(
                KeyCodeRepr::Char('H'),
                KeyModifiersRepr::CONTROL
            ))
        );
    }

    #[test]
    fn non_printable_chords_keep_shift() {
        let chord = key_chord_from_event(TuiKeyEvent {
            code: TuiKeyCode::Enter,
            modifiers: TuiKeyModifiers {
                control: false,
                shift: true,
                alt: false,
            },
        });

        assert_eq!(
            chord,
            Some(KeyChord::new(
                KeyCodeRepr::Enter,
                KeyModifiersRepr::new(false, true, false)
            ))
        );
    }
}
