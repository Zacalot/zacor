use crate::input::{
    Key, KeyDownEvent, KeyLocation, KeyUpEvent, KeyboardEvent, Keystroke, ModifierKey, Modifiers,
    NamedKey, PointerButton,
};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

pub fn convert_modifiers(modifiers: winit::keyboard::ModifiersState) -> Modifiers {
    Modifiers {
        shift: modifiers.shift_key(),
        control: modifiers.control_key(),
        alt: modifiers.alt_key(),
        platform: modifiers.super_key(),
        function: false,
    }
}

pub fn convert_keyboard_event(
    event: winit::event::KeyEvent,
    modifiers: Modifiers,
) -> KeyboardEvent {
    let keystroke = build_keystroke(
        event.key_without_modifiers(),
        event.text.as_deref(),
        modifiers,
        event.location,
    );

    match event.state {
        winit::event::ElementState::Pressed => KeyboardEvent::KeyDown(KeyDownEvent {
            keystroke,
            repeat: event.repeat,
            prefer_text: false,
        }),
        winit::event::ElementState::Released => KeyboardEvent::KeyUp(KeyUpEvent { keystroke }),
    }
}

pub fn convert_pointer_button(button: winit::event::MouseButton) -> Option<PointerButton> {
    match button {
        winit::event::MouseButton::Left => Some(PointerButton::Primary),
        winit::event::MouseButton::Right => Some(PointerButton::Secondary),
        winit::event::MouseButton::Middle => Some(PointerButton::Middle),
        winit::event::MouseButton::Back | winit::event::MouseButton::Forward => None,
        winit::event::MouseButton::Other(button) => Some(PointerButton::Other(button)),
    }
}

fn build_keystroke(
    binding_key: winit::keyboard::Key,
    text: Option<&str>,
    modifiers: Modifiers,
    location: winit::keyboard::KeyLocation,
) -> Keystroke {
    let key = convert_key(binding_key);
    Keystroke {
        text: normalize_text(text, modifiers, &key),
        key,
        modifiers,
        location: convert_key_location(location),
    }
}

fn normalize_text(text: Option<&str>, modifiers: Modifiers, key: &Key) -> Option<String> {
    if modifiers.control || modifiers.platform {
        return None;
    }

    match key {
        Key::Character(_) | Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Tab) => {
            text.map(ToOwned::to_owned)
        }
        Key::Named(NamedKey::Space) => text.map(ToOwned::to_owned),
        Key::Named(_) | Key::Function(_) | Key::Modifier(_) | Key::Dead(_) | Key::Unknown(_) => {
            None
        }
    }
}

fn convert_key(key: winit::keyboard::Key) -> Key {
    match key {
        winit::keyboard::Key::Character(text) => Key::Character(text.to_string()),
        winit::keyboard::Key::Named(key) => convert_named_key(key),
        winit::keyboard::Key::Dead(dead) => Key::Dead(dead),
        winit::keyboard::Key::Unidentified(native) => Key::Unknown(format!("{native:?}")),
    }
}

fn convert_named_key(key: winit::keyboard::NamedKey) -> Key {
    match key {
        winit::keyboard::NamedKey::Escape => Key::Named(NamedKey::Escape),
        winit::keyboard::NamedKey::Enter => Key::Named(NamedKey::Enter),
        winit::keyboard::NamedKey::Tab => Key::Named(NamedKey::Tab),
        winit::keyboard::NamedKey::Space => Key::Named(NamedKey::Space),
        winit::keyboard::NamedKey::Backspace => Key::Named(NamedKey::Backspace),
        winit::keyboard::NamedKey::Delete => Key::Named(NamedKey::Delete),
        winit::keyboard::NamedKey::Insert => Key::Named(NamedKey::Insert),
        winit::keyboard::NamedKey::ArrowUp => Key::Named(NamedKey::ArrowUp),
        winit::keyboard::NamedKey::ArrowDown => Key::Named(NamedKey::ArrowDown),
        winit::keyboard::NamedKey::ArrowLeft => Key::Named(NamedKey::ArrowLeft),
        winit::keyboard::NamedKey::ArrowRight => Key::Named(NamedKey::ArrowRight),
        winit::keyboard::NamedKey::Home => Key::Named(NamedKey::Home),
        winit::keyboard::NamedKey::End => Key::Named(NamedKey::End),
        winit::keyboard::NamedKey::PageUp => Key::Named(NamedKey::PageUp),
        winit::keyboard::NamedKey::PageDown => Key::Named(NamedKey::PageDown),
        winit::keyboard::NamedKey::Shift => Key::Modifier(ModifierKey::Shift),
        winit::keyboard::NamedKey::Control => Key::Modifier(ModifierKey::Control),
        winit::keyboard::NamedKey::Alt => Key::Modifier(ModifierKey::Alt),
        winit::keyboard::NamedKey::AltGraph => Key::Modifier(ModifierKey::AltGraph),
        winit::keyboard::NamedKey::Super => Key::Modifier(ModifierKey::Platform),
        winit::keyboard::NamedKey::Fn => Key::Modifier(ModifierKey::Function),
        winit::keyboard::NamedKey::CapsLock => Key::Modifier(ModifierKey::CapsLock),
        winit::keyboard::NamedKey::F1 => Key::Function(1),
        winit::keyboard::NamedKey::F2 => Key::Function(2),
        winit::keyboard::NamedKey::F3 => Key::Function(3),
        winit::keyboard::NamedKey::F4 => Key::Function(4),
        winit::keyboard::NamedKey::F5 => Key::Function(5),
        winit::keyboard::NamedKey::F6 => Key::Function(6),
        winit::keyboard::NamedKey::F7 => Key::Function(7),
        winit::keyboard::NamedKey::F8 => Key::Function(8),
        winit::keyboard::NamedKey::F9 => Key::Function(9),
        winit::keyboard::NamedKey::F10 => Key::Function(10),
        winit::keyboard::NamedKey::F11 => Key::Function(11),
        winit::keyboard::NamedKey::F12 => Key::Function(12),
        key => Key::Unknown(format!("{key:?}")),
    }
}

fn convert_key_location(location: winit::keyboard::KeyLocation) -> KeyLocation {
    match location {
        winit::keyboard::KeyLocation::Standard => KeyLocation::Standard,
        winit::keyboard::KeyLocation::Left => KeyLocation::Left,
        winit::keyboard::KeyLocation::Right => KeyLocation::Right,
        winit::keyboard::KeyLocation::Numpad => KeyLocation::Numpad,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_modifiers_sets_keyboard_modifier_state() {
        let modifiers = convert_modifiers(winit::keyboard::ModifiersState::SHIFT);

        assert!(modifiers.shift);
        assert!(!modifiers.control);
        assert!(!modifiers.alt);
        assert!(!modifiers.platform);
        assert!(!modifiers.function);
    }

    #[test]
    fn convert_named_key_maps_common_keys() {
        assert_eq!(
            convert_named_key(winit::keyboard::NamedKey::Escape),
            Key::Named(NamedKey::Escape)
        );
        assert_eq!(
            convert_named_key(winit::keyboard::NamedKey::F1),
            Key::Function(1)
        );
        assert_eq!(
            convert_named_key(winit::keyboard::NamedKey::Shift),
            Key::Modifier(ModifierKey::Shift)
        );
    }

    #[test]
    fn build_keystroke_keeps_shifted_text() {
        let keystroke = build_keystroke(
            winit::keyboard::Key::Character("a".into()),
            Some("A"),
            Modifiers {
                shift: true,
                control: false,
                alt: false,
                platform: false,
                function: false,
            },
            winit::keyboard::KeyLocation::Standard,
        );

        assert_eq!(keystroke.key, Key::Character("a".to_string()));
        assert_eq!(keystroke.text, Some("A".to_string()));
    }

    #[test]
    fn build_keystroke_suppresses_text_for_command_chords() {
        let keystroke = build_keystroke(
            winit::keyboard::Key::Character("a".into()),
            Some("a"),
            Modifiers {
                shift: false,
                control: true,
                alt: false,
                platform: false,
                function: false,
            },
            winit::keyboard::KeyLocation::Standard,
        );

        assert_eq!(keystroke.text, None);
    }
}
