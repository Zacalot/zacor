use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeymapRegistry {
    bindings: BTreeMap<Vec<KeyChord>, String>,
}

impl KeymapRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind(&mut self, sequence: impl Into<Vec<KeyChord>>, command: impl Into<String>) {
        self.bindings.insert(sequence.into(), command.into());
    }

    pub fn lookup(&self, sequence: &[KeyChord]) -> KeymapLookup<'_> {
        if let Some(command) = self.bindings.get(sequence) {
            return KeymapLookup::Matched(command.as_str());
        }

        if self
            .bindings
            .keys()
            .any(|binding| binding.starts_with(sequence))
        {
            KeymapLookup::Pending
        } else {
            KeymapLookup::NoMatch
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct KeyChord {
    code: KeyCodeRepr,
    modifiers: KeyModifiersRepr,
}

impl KeyChord {
    pub fn new(code: KeyCodeRepr, modifiers: KeyModifiersRepr) -> Self {
        Self { code, modifiers }
    }

    pub fn code(&self) -> KeyCodeRepr {
        self.code
    }

    pub fn modifiers(&self) -> KeyModifiersRepr {
        self.modifiers
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum KeyCodeRepr {
    Backspace,
    Enter,
    Esc,
    Char(char),
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct KeyModifiersRepr {
    control: bool,
    shift: bool,
    alt: bool,
}

impl KeyModifiersRepr {
    pub const fn new(control: bool, shift: bool, alt: bool) -> Self {
        Self {
            control,
            shift,
            alt,
        }
    }

    pub const NONE: Self = Self {
        control: false,
        shift: false,
        alt: false,
    };

    pub const CONTROL: Self = Self {
        control: true,
        shift: false,
        alt: false,
    };

    pub fn control(&self) -> bool {
        self.control
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeymapLookup<'a> {
    Matched(&'a str),
    Pending,
    NoMatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_distinguishes_match_pending_and_miss() {
        let mut keymap = KeymapRegistry::new();
        keymap.bind(
            vec![
                KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
                KeyChord::new(KeyCodeRepr::Char('v'), KeyModifiersRepr::NONE),
            ],
            "pane.split.vertical",
        );

        assert_eq!(
            keymap.lookup(&[KeyChord::new(
                KeyCodeRepr::Char('w'),
                KeyModifiersRepr::CONTROL,
            )]),
            KeymapLookup::Pending
        );
        assert_eq!(
            keymap.lookup(&[
                KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
                KeyChord::new(KeyCodeRepr::Char('v'), KeyModifiersRepr::NONE),
            ]),
            KeymapLookup::Matched("pane.split.vertical")
        );
        assert_eq!(
            keymap.lookup(&[KeyChord::new(
                KeyCodeRepr::Char('x'),
                KeyModifiersRepr::NONE
            )]),
            KeymapLookup::NoMatch
        );
    }
}
