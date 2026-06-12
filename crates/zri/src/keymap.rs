use std::collections::BTreeMap;

use crate::function::FunctionName;
use crate::input::{Key, KeyDownEvent, KeyLocation, KeyboardEvent, Keystroke, Modifiers, NamedKey};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeymapError {
    message: String,
}

impl KeymapError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BindingChord {
    pub key: Key,
    pub modifiers: Modifiers,
    pub location: KeyLocation,
}

impl BindingChord {
    pub fn from_keystroke(keystroke: &Keystroke) -> Self {
        Self {
            key: keystroke.key.clone(),
            modifiers: keystroke.modifiers,
            location: keystroke.location,
        }
    }

    pub fn from_key_down(event: &KeyDownEvent) -> Self {
        Self::from_keystroke(&event.keystroke)
    }

    pub fn is_modifier_only(&self) -> bool {
        matches!(self.key, Key::Modifier(_))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeySequence {
    chords: Vec<BindingChord>,
}

impl KeySequence {
    pub fn new(chords: Vec<BindingChord>) -> Result<Self, KeymapError> {
        if chords.is_empty() {
            return Err(KeymapError::new("key sequence must not be empty"));
        }
        Ok(Self { chords })
    }

    pub fn chords(&self) -> &[BindingChord] {
        &self.chords
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyBinding {
    pub sequence: KeySequence,
    pub function: FunctionName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveKeymap<'a> {
    pub name: &'a str,
    pub keymap: &'a Keymap,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeymapResolution {
    Ignored,
    Pending {
        sequence: KeySequence,
    },
    Matched {
        sequence: KeySequence,
        function: FunctionName,
    },
    /// No binding matched. `events` carries every swallowed key-down
    /// (pending prefix plus the failing key) so the caller can replay them —
    /// single-key binding lookup first, then literal insertion for
    /// text-bearing keys, then drop. Never silently discard: under the
    /// insertion-as-fallback model, discarded events are eaten user text.
    NotFound {
        sequence: KeySequence,
        events: Vec<KeyDownEvent>,
    },
    /// Escape cancelled a pending sequence. `events` carries the swallowed
    /// prefix plus the escape key-down for the same replay contract
    /// (Helix insert-mode precedent: cancelled chord keys are replayed).
    Cancelled {
        sequence: KeySequence,
        events: Vec<KeyDownEvent>,
    },
}

impl KeymapResolution {
    pub fn matched_function(&self) -> Option<&FunctionName> {
        match self {
            Self::Matched { function, .. } => Some(function),
            Self::Ignored
            | Self::Pending { .. }
            | Self::NotFound { .. }
            | Self::Cancelled { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Keymap {
    root: KeymapNode,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct KeymapNode {
    binding: Option<FunctionName>,
    children: BTreeMap<BindingChord, KeymapNode>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeymapMatch<'a> {
    Matched(&'a FunctionName),
    Pending,
    NotFound,
}

impl Keymap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind a sequence, rejecting prefix/exact conflicts at bind time (the
    /// Emacs `define-key` / Helix node-or-leaf discipline). A node is either a
    /// prefix or a binding, never both, which makes ambiguity — and therefore
    /// timeout machinery — structurally impossible. Rebinding the identical
    /// sequence replaces the previous function.
    pub fn bind(
        &mut self,
        sequence: KeySequence,
        function: FunctionName,
    ) -> Result<(), KeymapError> {
        // Validate against the existing trie before mutating, so a rejected
        // bind never leaves orphan prefix nodes behind.
        let chords = sequence.chords();
        let mut node = Some(&self.root);
        for (index, chord) in chords.iter().enumerate() {
            let Some(next) = node.and_then(|current| current.children.get(chord)) else {
                // The rest of the path is fresh; no conflicts are possible.
                node = None;
                break;
            };
            let is_last = index + 1 == chords.len();
            if !is_last && next.binding.is_some() {
                return Err(KeymapError::new(
                    "key sequence passes through an existing binding",
                ));
            }
            if is_last && !next.children.is_empty() {
                return Err(KeymapError::new(
                    "key sequence is a prefix of an existing binding",
                ));
            }
            node = Some(next);
        }
        let _ = node;

        let mut node = &mut self.root;
        for chord in sequence.chords {
            node = node.children.entry(chord).or_default();
        }
        node.binding = Some(function);
        Ok(())
    }

    /// Exact single-chord binding lookup, used by the replay path. A chord
    /// that is only a prefix does not match: replaying a key must never
    /// re-enter pending state.
    pub fn binding_for_chord(&self, chord: &BindingChord) -> Option<&FunctionName> {
        self.root
            .children
            .get(chord)
            .and_then(|node| node.binding.as_ref())
    }

    fn lookup(&self, sequence: &[BindingChord]) -> KeymapMatch<'_> {
        let mut node = &self.root;
        for chord in sequence {
            let Some(next) = node.children.get(chord) else {
                return KeymapMatch::NotFound;
            };
            node = next;
        }

        if let Some(function) = &node.binding {
            KeymapMatch::Matched(function)
        } else if node.children.is_empty() {
            KeymapMatch::NotFound
        } else {
            KeymapMatch::Pending
        }
    }
}

/// Pending state buffers full key-down events (not just binding chords) so a
/// failed sequence can replay the swallowed keys with their produced text
/// intact.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeymapResolver {
    pending: Vec<KeyDownEvent>,
}

impl KeymapResolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending_chords(&self) -> Vec<BindingChord> {
        self.pending
            .iter()
            .map(BindingChord::from_key_down)
            .collect()
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Drop any pending sequence. Must be called on focus change (Zed
    /// precedent: pending keystrokes are invalidated when focus moves) —
    /// a prefix typed in one context must not resolve in another.
    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }

    pub fn resolve(
        &mut self,
        event: &KeyboardEvent,
        active_keymaps: &[ActiveKeymap<'_>],
    ) -> KeymapResolution {
        let KeyboardEvent::KeyDown(key_down) = event else {
            return KeymapResolution::Ignored;
        };

        let chord = BindingChord::from_key_down(key_down);
        if chord.is_modifier_only() {
            return KeymapResolution::Ignored;
        }

        if !self.pending.is_empty() && chord.key == Key::Named(NamedKey::Escape) {
            let mut events = std::mem::take(&mut self.pending);
            events.push(key_down.clone());
            return KeymapResolution::Cancelled {
                sequence: sequence_of(&events),
                events,
            };
        }

        let mut attempted_chords = self.pending_chords();
        attempted_chords.push(chord);

        for active_keymap in active_keymaps {
            match active_keymap.keymap.lookup(&attempted_chords) {
                KeymapMatch::Matched(function) => {
                    self.pending.clear();
                    return KeymapResolution::Matched {
                        sequence: KeySequence {
                            chords: attempted_chords,
                        },
                        function: function.clone(),
                    };
                }
                KeymapMatch::Pending => {
                    self.pending.push(key_down.clone());
                    return KeymapResolution::Pending {
                        sequence: KeySequence {
                            chords: attempted_chords,
                        },
                    };
                }
                KeymapMatch::NotFound => {}
            }
        }

        let mut events = std::mem::take(&mut self.pending);
        events.push(key_down.clone());
        KeymapResolution::NotFound {
            sequence: KeySequence {
                chords: attempted_chords,
            },
            events,
        }
    }
}

fn sequence_of(events: &[KeyDownEvent]) -> KeySequence {
    KeySequence {
        chords: events.iter().map(BindingChord::from_key_down).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{KeyUpEvent, ModifierKey, ModifiersChangedEvent};

    fn char_chord(ch: &str) -> BindingChord {
        BindingChord {
            key: Key::Character(ch.to_string()),
            modifiers: Modifiers::default(),
            location: KeyLocation::Standard,
        }
    }

    fn escape_chord() -> BindingChord {
        BindingChord {
            key: Key::Named(NamedKey::Escape),
            modifiers: Modifiers::default(),
            location: KeyLocation::Standard,
        }
    }

    fn key_down_event(key: Key, text: Option<&str>) -> KeyDownEvent {
        KeyDownEvent {
            keystroke: Keystroke {
                key,
                text: text.map(ToOwned::to_owned),
                modifiers: Modifiers::default(),
                location: KeyLocation::Standard,
            },
            repeat: false,
            prefer_text: false,
        }
    }

    fn key_down(key: Key, text: Option<&str>) -> KeyboardEvent {
        KeyboardEvent::KeyDown(key_down_event(key, text))
    }

    fn shifted_key_down(key: &str, text: &str) -> KeyboardEvent {
        KeyboardEvent::KeyDown(KeyDownEvent {
            keystroke: Keystroke {
                key: Key::Character(key.to_string()),
                text: Some(text.to_string()),
                modifiers: Modifiers {
                    shift: true,
                    control: false,
                    alt: false,
                    platform: false,
                    function: false,
                },
                location: KeyLocation::Standard,
            },
            repeat: false,
            prefer_text: false,
        })
    }

    fn bind(map: &mut Keymap, chords: Vec<BindingChord>, function: &str) {
        map.bind(
            KeySequence::new(chords).unwrap(),
            FunctionName::new(function).unwrap(),
        )
        .unwrap();
    }

    fn active<'a>(name: &'a str, keymap: &'a Keymap) -> ActiveKeymap<'a> {
        ActiveKeymap { name, keymap }
    }

    #[test]
    fn binding_chord_ignores_produced_text() {
        let event = shifted_key_down("a", "A");
        let KeyboardEvent::KeyDown(key_down) = event else {
            unreachable!();
        };

        let chord = BindingChord::from_key_down(&key_down);

        assert_eq!(chord.key, Key::Character("a".to_string()));
        assert!(chord.modifiers.shift);
    }

    #[test]
    fn key_sequence_rejects_empty_sequences() {
        let error = KeySequence::new(Vec::new()).unwrap_err();
        assert_eq!(error.message(), "key sequence must not be empty");
    }

    #[test]
    fn key_down_resolves_single_key_binding() {
        let mut map = Keymap::new();
        bind(&mut map, vec![char_chord("a")], "demo.hello");
        let mut resolver = KeymapResolver::new();

        let resolution = resolver.resolve(
            &key_down(Key::Character("a".to_string()), Some("a")),
            &[active("global", &map)],
        );

        assert_eq!(
            resolution,
            KeymapResolution::Matched {
                sequence: KeySequence::new(vec![char_chord("a")]).unwrap(),
                function: FunctionName::new("demo.hello").unwrap(),
            }
        );
        assert!(!resolver.has_pending());
    }

    #[test]
    fn key_up_is_ignored() {
        let mut resolver = KeymapResolver::new();
        let event = KeyboardEvent::KeyUp(KeyUpEvent {
            keystroke: Keystroke {
                key: Key::Character("a".to_string()),
                text: Some("a".to_string()),
                modifiers: Modifiers::default(),
                location: KeyLocation::Standard,
            },
        });

        assert_eq!(resolver.resolve(&event, &[]), KeymapResolution::Ignored);
    }

    #[test]
    fn modifiers_changed_is_ignored() {
        let mut resolver = KeymapResolver::new();
        let event = KeyboardEvent::ModifiersChanged(ModifiersChangedEvent {
            modifiers: Modifiers::default(),
        });

        assert_eq!(resolver.resolve(&event, &[]), KeymapResolution::Ignored);
    }

    #[test]
    fn modifier_key_is_ignored() {
        let mut resolver = KeymapResolver::new();

        let resolution = resolver.resolve(&key_down(Key::Modifier(ModifierKey::Shift), None), &[]);

        assert_eq!(resolution, KeymapResolution::Ignored);
    }

    #[test]
    fn prefix_returns_pending() {
        let mut map = Keymap::new();
        bind(
            &mut map,
            vec![char_chord("g"), char_chord("d")],
            "demo.goto",
        );
        let mut resolver = KeymapResolver::new();

        let resolution = resolver.resolve(
            &key_down(Key::Character("g".to_string()), Some("g")),
            &[active("global", &map)],
        );

        assert_eq!(
            resolution,
            KeymapResolution::Pending {
                sequence: KeySequence::new(vec![char_chord("g")]).unwrap(),
            }
        );
        assert_eq!(resolver.pending_chords(), vec![char_chord("g")]);
    }

    #[test]
    fn second_key_matches_pending_sequence() {
        let mut map = Keymap::new();
        bind(
            &mut map,
            vec![char_chord("g"), char_chord("d")],
            "demo.goto",
        );
        let mut resolver = KeymapResolver::new();
        let _ = resolver.resolve(
            &key_down(Key::Character("g".to_string()), Some("g")),
            &[active("global", &map)],
        );

        let resolution = resolver.resolve(
            &key_down(Key::Character("d".to_string()), Some("d")),
            &[active("global", &map)],
        );

        assert_eq!(
            resolution,
            KeymapResolution::Matched {
                sequence: KeySequence::new(vec![char_chord("g"), char_chord("d")]).unwrap(),
                function: FunctionName::new("demo.goto").unwrap(),
            }
        );
        assert!(!resolver.has_pending());
    }

    #[test]
    fn escape_cancels_pending_sequence_and_returns_swallowed_events() {
        let mut map = Keymap::new();
        bind(
            &mut map,
            vec![char_chord("g"), char_chord("d")],
            "demo.goto",
        );
        let mut resolver = KeymapResolver::new();
        let _ = resolver.resolve(
            &key_down(Key::Character("g".to_string()), Some("g")),
            &[active("global", &map)],
        );

        let resolution = resolver.resolve(
            &key_down(Key::Named(NamedKey::Escape), None),
            &[active("global", &map)],
        );

        assert_eq!(
            resolution,
            KeymapResolution::Cancelled {
                sequence: KeySequence::new(vec![char_chord("g"), escape_chord()]).unwrap(),
                events: vec![
                    key_down_event(Key::Character("g".to_string()), Some("g")),
                    key_down_event(Key::Named(NamedKey::Escape), None),
                ],
            }
        );
        assert!(!resolver.has_pending());
    }

    #[test]
    fn not_found_clears_pending_sequence_and_returns_swallowed_events() {
        let mut map = Keymap::new();
        bind(
            &mut map,
            vec![char_chord("g"), char_chord("d")],
            "demo.goto",
        );
        let mut resolver = KeymapResolver::new();
        let _ = resolver.resolve(
            &key_down(Key::Character("g".to_string()), Some("g")),
            &[active("global", &map)],
        );

        let resolution = resolver.resolve(
            &key_down(Key::Character("x".to_string()), Some("x")),
            &[active("global", &map)],
        );

        assert_eq!(
            resolution,
            KeymapResolution::NotFound {
                sequence: KeySequence::new(vec![char_chord("g"), char_chord("x")]).unwrap(),
                events: vec![
                    key_down_event(Key::Character("g".to_string()), Some("g")),
                    key_down_event(Key::Character("x".to_string()), Some("x")),
                ],
            }
        );
        assert!(!resolver.has_pending());
    }

    #[test]
    fn clear_pending_drops_sequence_state() {
        let mut map = Keymap::new();
        bind(
            &mut map,
            vec![char_chord("g"), char_chord("d")],
            "demo.goto",
        );
        let mut resolver = KeymapResolver::new();
        let _ = resolver.resolve(
            &key_down(Key::Character("g".to_string()), Some("g")),
            &[active("global", &map)],
        );
        assert!(resolver.has_pending());

        resolver.clear_pending();

        assert!(!resolver.has_pending());
        // After the clear, "d" alone is unbound: it must not complete the
        // dropped sequence.
        let resolution = resolver.resolve(
            &key_down(Key::Character("d".to_string()), Some("d")),
            &[active("global", &map)],
        );
        assert!(matches!(resolution, KeymapResolution::NotFound { .. }));
    }

    #[test]
    fn bind_rejects_sequence_passing_through_existing_binding() {
        let mut map = Keymap::new();
        bind(&mut map, vec![char_chord("g")], "demo.first");

        let error = map
            .bind(
                KeySequence::new(vec![char_chord("g"), char_chord("d")]).unwrap(),
                FunctionName::new("demo.second").unwrap(),
            )
            .unwrap_err();

        assert_eq!(
            error.message(),
            "key sequence passes through an existing binding"
        );
    }

    #[test]
    fn bind_rejects_sequence_that_is_prefix_of_existing_binding() {
        let mut map = Keymap::new();
        bind(
            &mut map,
            vec![char_chord("g"), char_chord("d")],
            "demo.goto",
        );

        let error = map
            .bind(
                KeySequence::new(vec![char_chord("g")]).unwrap(),
                FunctionName::new("demo.shadow").unwrap(),
            )
            .unwrap_err();

        assert_eq!(
            error.message(),
            "key sequence is a prefix of an existing binding"
        );
    }

    #[test]
    fn rejected_bind_leaves_no_orphan_prefix_state() {
        let mut map = Keymap::new();
        bind(&mut map, vec![char_chord("g")], "demo.first");
        let _ = map.bind(
            KeySequence::new(vec![char_chord("g"), char_chord("d"), char_chord("x")]).unwrap(),
            FunctionName::new("demo.rejected").unwrap(),
        );

        // The rejected bind must not have created prefix nodes that would make
        // "g" report Pending instead of its exact match.
        let mut resolver = KeymapResolver::new();
        let resolution = resolver.resolve(
            &key_down(Key::Character("g".to_string()), Some("g")),
            &[active("global", &map)],
        );

        assert_eq!(
            resolution.matched_function(),
            Some(&FunctionName::new("demo.first").unwrap())
        );
    }

    #[test]
    fn binding_for_chord_matches_exact_bindings_only() {
        let mut map = Keymap::new();
        bind(&mut map, vec![char_chord("x")], "demo.exact");
        bind(
            &mut map,
            vec![char_chord("g"), char_chord("d")],
            "demo.goto",
        );

        assert_eq!(
            map.binding_for_chord(&char_chord("x")),
            Some(&FunctionName::new("demo.exact").unwrap())
        );
        // "g" is only a prefix: replay lookups must not re-enter pending.
        assert_eq!(map.binding_for_chord(&char_chord("g")), None);
        assert_eq!(map.binding_for_chord(&char_chord("z")), None);
    }

    #[test]
    fn higher_precedence_pending_beats_lower_precedence_match() {
        let mut focus_map = Keymap::new();
        bind(
            &mut focus_map,
            vec![char_chord("g"), char_chord("d")],
            "demo.focus",
        );
        let mut global_map = Keymap::new();
        bind(&mut global_map, vec![char_chord("g")], "demo.global");
        let mut resolver = KeymapResolver::new();

        let resolution = resolver.resolve(
            &key_down(Key::Character("g".to_string()), Some("g")),
            &[active("focus", &focus_map), active("global", &global_map)],
        );

        assert_eq!(
            resolution,
            KeymapResolution::Pending {
                sequence: KeySequence::new(vec![char_chord("g")]).unwrap(),
            }
        );
    }

    #[test]
    fn higher_precedence_match_beats_lower_precedence_match() {
        let mut focus_map = Keymap::new();
        bind(&mut focus_map, vec![char_chord("x")], "demo.focus");
        let mut global_map = Keymap::new();
        bind(&mut global_map, vec![char_chord("x")], "demo.global");
        let mut resolver = KeymapResolver::new();

        let resolution = resolver.resolve(
            &key_down(Key::Character("x".to_string()), Some("x")),
            &[active("focus", &focus_map), active("global", &global_map)],
        );

        assert_eq!(
            resolution,
            KeymapResolution::Matched {
                sequence: KeySequence::new(vec![char_chord("x")]).unwrap(),
                function: FunctionName::new("demo.focus").unwrap(),
            }
        );
    }

    #[test]
    fn matched_function_returns_function_for_matched_resolution() {
        let function = FunctionName::new("demo.focus").unwrap();
        let resolution = KeymapResolution::Matched {
            sequence: KeySequence::new(vec![char_chord("x")]).unwrap(),
            function: function.clone(),
        };

        assert_eq!(resolution.matched_function(), Some(&function));
    }

    #[test]
    fn matched_function_returns_none_for_non_matched_resolutions() {
        let pending = KeymapResolution::Pending {
            sequence: KeySequence::new(vec![char_chord("g")]).unwrap(),
        };
        let not_found = KeymapResolution::NotFound {
            sequence: KeySequence::new(vec![char_chord("z")]).unwrap(),
            events: vec![key_down_event(Key::Character("z".to_string()), Some("z"))],
        };
        let cancelled = KeymapResolution::Cancelled {
            sequence: KeySequence::new(vec![char_chord("g"), escape_chord()]).unwrap(),
            events: vec![
                key_down_event(Key::Character("g".to_string()), Some("g")),
                key_down_event(Key::Named(NamedKey::Escape), None),
            ],
        };

        assert_eq!(KeymapResolution::Ignored.matched_function(), None);
        assert_eq!(pending.matched_function(), None);
        assert_eq!(not_found.matched_function(), None);
        assert_eq!(cancelled.matched_function(), None);
    }

    #[test]
    fn same_sequence_binding_replaces_previous_function() {
        let mut map = Keymap::new();
        bind(&mut map, vec![char_chord("x")], "demo.first");
        bind(&mut map, vec![char_chord("x")], "demo.second");
        let mut resolver = KeymapResolver::new();

        let resolution = resolver.resolve(
            &key_down(Key::Character("x".to_string()), Some("x")),
            &[active("global", &map)],
        );

        assert_eq!(
            resolution,
            KeymapResolution::Matched {
                sequence: KeySequence::new(vec![char_chord("x")]).unwrap(),
                function: FunctionName::new("demo.second").unwrap(),
            }
        );
    }

    #[test]
    fn unbound_key_returns_not_found() {
        let map = Keymap::new();
        let mut resolver = KeymapResolver::new();

        let resolution = resolver.resolve(
            &key_down(Key::Character("z".to_string()), Some("z")),
            &[active("global", &map)],
        );

        assert_eq!(
            resolution,
            KeymapResolution::NotFound {
                sequence: KeySequence::new(vec![BindingChord {
                    key: Key::Character("z".to_string()),
                    modifiers: Modifiers::default(),
                    location: KeyLocation::Standard,
                }])
                .unwrap(),
                events: vec![key_down_event(Key::Character("z".to_string()), Some("z"))],
            }
        );
    }
}
