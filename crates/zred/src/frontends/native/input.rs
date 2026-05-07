use crate::kernel::{KeyChord, KeyCodeRepr, KeyModifiersRepr};
use crate::session::AppInputEvent;
use winit::event::ElementState;
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};

pub fn app_input_event_from_winit(
    event: &winit::event::KeyEvent,
    modifiers: ModifiersState,
) -> Option<AppInputEvent> {
    if event.state != ElementState::Pressed || event.repeat {
        return None;
    }

    let key_chord = key_chord_from_winit(event, modifiers)?;
    let text_input = text_input_from_winit(event, modifiers);
    Some(AppInputEvent::new(Some(key_chord), text_input))
}

fn key_chord_from_winit(
    event: &winit::event::KeyEvent,
    modifiers: ModifiersState,
) -> Option<KeyChord> {
    if let Some(ch) = printable_char_from_winit(event, modifiers) {
        return Some(KeyChord::new(
            KeyCodeRepr::Char(ch),
            printable_modifiers(modifiers),
        ));
    }

    let code = match &event.logical_key {
        Key::Named(NamedKey::Enter) => KeyCodeRepr::Enter,
        Key::Named(NamedKey::Escape) => KeyCodeRepr::Esc,
        Key::Named(NamedKey::Backspace) => KeyCodeRepr::Backspace,
        _ => match event.physical_key {
            PhysicalKey::Code(KeyCode::Enter) => KeyCodeRepr::Enter,
            PhysicalKey::Code(KeyCode::Escape) => KeyCodeRepr::Esc,
            PhysicalKey::Code(KeyCode::Backspace) => KeyCodeRepr::Backspace,
            _ => return None,
        },
    };

    Some(KeyChord::new(code, non_printable_modifiers(modifiers)))
}

fn printable_modifiers(modifiers: ModifiersState) -> KeyModifiersRepr {
    KeyModifiersRepr::new(modifiers.control_key(), false, modifiers.alt_key())
}

fn printable_char_from_winit(
    event: &winit::event::KeyEvent,
    modifiers: ModifiersState,
) -> Option<char> {
    printable_char_from_sources(
        event.logical_key_text(),
        event.text.as_deref(),
        modifiers.control_key(),
        modifiers.alt_key(),
    )
}

fn printable_char_from_sources(
    logical_text: Option<&str>,
    event_text: Option<&str>,
    control: bool,
    alt: bool,
) -> Option<char> {
    if control || alt {
        return logical_text.and_then(first_char);
    }

    event_text.or(logical_text).and_then(first_char)
}

fn first_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let ch = chars.next()?;
    chars.next().is_none().then_some(ch)
}

fn non_printable_modifiers(modifiers: ModifiersState) -> KeyModifiersRepr {
    KeyModifiersRepr::new(
        modifiers.control_key(),
        modifiers.shift_key(),
        modifiers.alt_key(),
    )
}

fn text_input_from_winit(
    event: &winit::event::KeyEvent,
    modifiers: ModifiersState,
) -> Option<char> {
    printable_char_from_sources(
        event.logical_key_text(),
        event.text.as_deref(),
        modifiers.control_key(),
        modifiers.alt_key(),
    )
}

trait LogicalKeyText {
    fn logical_key_text(&self) -> Option<&str>;
}

impl LogicalKeyText for winit::event::KeyEvent {
    fn logical_key_text(&self) -> Option<&str> {
        match &self.logical_key {
            Key::Character(text) => Some(text.as_str()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printable_modifiers_drop_shift_but_keep_control_and_alt() {
        let modifiers = ModifiersState::SHIFT | ModifiersState::CONTROL | ModifiersState::ALT;

        assert_eq!(
            printable_modifiers(modifiers),
            KeyModifiersRepr::new(true, false, true)
        );
    }

    #[test]
    fn non_printable_modifiers_keep_shift() {
        let modifiers = ModifiersState::SHIFT;

        assert_eq!(
            non_printable_modifiers(modifiers),
            KeyModifiersRepr::new(false, true, false)
        );
    }

    #[test]
    fn printable_char_prefers_event_text_for_shifted_input() {
        assert_eq!(
            printable_char_from_sources(Some(";"), Some(":"), false, false),
            Some(':')
        );
    }

    #[test]
    fn printable_char_falls_back_to_logical_text_when_event_text_missing() {
        assert_eq!(
            printable_char_from_sources(Some("h"), None, false, false),
            Some('h')
        );
    }

    #[test]
    fn printable_char_ignores_control_text_input() {
        assert_eq!(printable_char_from_sources(Some("w"), Some("\u{17}"), true, false), Some('w'));
    }
}
