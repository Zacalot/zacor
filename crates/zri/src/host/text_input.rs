use crate::input::{Key, KeyboardEvent, NamedKey};

use super::{BufferId, BufferKind, InterfaceHost, ViewCursor, ViewId};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextInputResult {
    pub changed: bool,
    pub view: Option<ViewId>,
    pub buffer: Option<BufferId>,
}

impl TextInputResult {
    fn unchanged(view: Option<ViewId>, buffer: Option<BufferId>) -> Self {
        Self {
            changed: false,
            view,
            buffer,
        }
    }

    fn changed(view: ViewId, buffer: BufferId) -> Self {
        Self {
            changed: true,
            view: Some(view),
            buffer: Some(buffer),
        }
    }
}

pub fn apply_keyboard_event(host: &mut InterfaceHost, event: &KeyboardEvent) -> TextInputResult {
    let KeyboardEvent::KeyDown(event) = event else {
        return TextInputResult::unchanged(host.selected_view(), None);
    };

    let Some(view_id) = host.selected_view() else {
        return TextInputResult::unchanged(None, None);
    };
    let Some(view) = host.view(view_id) else {
        return TextInputResult::unchanged(Some(view_id), None);
    };

    let buffer_id = view.buffer();
    let cursor = view.cursor();
    let Some(buffer) = host.buffer(buffer_id) else {
        return TextInputResult::unchanged(Some(view_id), Some(buffer_id));
    };
    if buffer.kind() != BufferKind::Text {
        return TextInputResult::unchanged(Some(view_id), Some(buffer_id));
    }

    let Some(edit) = edit_from_key(event.keystroke.text.as_deref(), &event.keystroke.key) else {
        return TextInputResult::unchanged(Some(view_id), Some(buffer_id));
    };

    let mut text = buffer.text().to_string();
    let Some(next_cursor) = apply_edit(&mut text, cursor, edit) else {
        return TextInputResult::unchanged(Some(view_id), Some(buffer_id));
    };

    if let Some(buffer) = host.buffer_mut(buffer_id) {
        buffer.set_text(text);
    }
    if let Some(view) = host.view_mut(view_id) {
        view.set_cursor(next_cursor);
    }

    TextInputResult::changed(view_id, buffer_id)
}

enum TextEdit<'a> {
    Insert(&'a str),
    Backspace,
}

fn edit_from_key<'a>(text: Option<&'a str>, key: &Key) -> Option<TextEdit<'a>> {
    match key {
        Key::Named(NamedKey::Backspace) => Some(TextEdit::Backspace),
        Key::Named(NamedKey::Enter) => Some(TextEdit::Insert("\n")),
        Key::Named(NamedKey::Tab) => Some(TextEdit::Insert("\t")),
        _ => text.filter(|text| !text.is_empty()).map(TextEdit::Insert),
    }
}

fn apply_edit(text: &mut String, cursor: ViewCursor, edit: TextEdit<'_>) -> Option<ViewCursor> {
    match edit {
        TextEdit::Insert(inserted) => {
            let offset = byte_offset_for_cursor(text, cursor)?;
            text.insert_str(offset, inserted);
            Some(cursor_after_insert(cursor, inserted))
        }
        TextEdit::Backspace => {
            let cursor_offset = byte_offset_for_cursor(text, cursor)?;
            let previous_offset = previous_char_offset(text, cursor_offset)?;
            text.replace_range(previous_offset..cursor_offset, "");
            cursor_for_byte_offset(text, previous_offset)
        }
    }
}

fn byte_offset_for_cursor(text: &str, cursor: ViewCursor) -> Option<usize> {
    let mut line = 0;
    let mut column = 0;

    for (offset, character) in text.char_indices() {
        if line == cursor.line && column == cursor.column {
            return Some(offset);
        }

        if character == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }

    if line == cursor.line && column == cursor.column {
        Some(text.len())
    } else {
        None
    }
}

fn previous_char_offset(text: &str, cursor_offset: usize) -> Option<usize> {
    if cursor_offset == 0 {
        return None;
    }

    text[..cursor_offset]
        .char_indices()
        .last()
        .map(|(offset, _)| offset)
}

fn cursor_for_byte_offset(text: &str, target_offset: usize) -> Option<ViewCursor> {
    if target_offset > text.len() || !text.is_char_boundary(target_offset) {
        return None;
    }

    let mut cursor = ViewCursor::default();
    for (offset, character) in text.char_indices() {
        if offset == target_offset {
            return Some(cursor);
        }
        if character == '\n' {
            cursor.line += 1;
            cursor.column = 0;
        } else {
            cursor.column += 1;
        }
    }

    if target_offset == text.len() {
        Some(cursor)
    } else {
        None
    }
}

fn cursor_after_insert(mut cursor: ViewCursor, inserted: &str) -> ViewCursor {
    for character in inserted.chars() {
        if character == '\n' {
            cursor.line += 1;
            cursor.column = 0;
        } else {
            cursor.column += 1;
        }
    }
    cursor
}

#[cfg(test)]
mod tests {
    use crate::host::{Pane, PaneContent, PaneId, PaneNode, PaneTree};
    use crate::input::{KeyDownEvent, KeyLocation, Keystroke, Modifiers};

    use super::*;

    const PANE: PaneId = PaneId(1);

    fn host_with_selected_buffer(
        kind: BufferKind,
        text: &str,
    ) -> (InterfaceHost, BufferId, ViewId) {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(PANE)));
        let buffer = host.create_buffer_with_text(kind, "buffer", text);
        let view = host.create_view(buffer);
        host.insert_pane(Pane::new(PANE, PaneContent::empty()).with_view(view));
        host.set_active_pane(Some(PANE));
        (host, buffer, view)
    }

    fn key_down(key: Key, text: Option<&str>) -> KeyboardEvent {
        KeyboardEvent::KeyDown(KeyDownEvent {
            keystroke: Keystroke {
                key,
                text: text.map(ToOwned::to_owned),
                modifiers: Modifiers::default(),
                location: KeyLocation::Standard,
            },
            repeat: false,
            prefer_text: false,
        })
    }

    #[test]
    fn inserts_printable_text_into_selected_text_buffer() {
        let (mut host, buffer, view) = host_with_selected_buffer(BufferKind::Text, "");

        let result =
            apply_keyboard_event(&mut host, &key_down(Key::Character("a".into()), Some("a")));

        assert!(result.changed);
        assert_eq!(host.buffer(buffer).unwrap().text(), "a");
        assert_eq!(
            host.view(view).unwrap().cursor(),
            ViewCursor { line: 0, column: 1 }
        );
    }

    #[test]
    fn enter_inserts_newline_and_updates_cursor() {
        let (mut host, buffer, view) = host_with_selected_buffer(BufferKind::Text, "a");
        host.view_mut(view)
            .unwrap()
            .set_cursor(ViewCursor { line: 0, column: 1 });

        let result = apply_keyboard_event(&mut host, &key_down(Key::Named(NamedKey::Enter), None));

        assert!(result.changed);
        assert_eq!(host.buffer(buffer).unwrap().text(), "a\n");
        assert_eq!(
            host.view(view).unwrap().cursor(),
            ViewCursor { line: 1, column: 0 }
        );
    }

    #[test]
    fn backspace_deletes_before_cursor() {
        let (mut host, buffer, view) = host_with_selected_buffer(BufferKind::Text, "ab");
        host.view_mut(view)
            .unwrap()
            .set_cursor(ViewCursor { line: 0, column: 2 });

        let result =
            apply_keyboard_event(&mut host, &key_down(Key::Named(NamedKey::Backspace), None));

        assert!(result.changed);
        assert_eq!(host.buffer(buffer).unwrap().text(), "a");
        assert_eq!(
            host.view(view).unwrap().cursor(),
            ViewCursor { line: 0, column: 1 }
        );
    }

    #[test]
    fn ignores_non_text_buffers() {
        let (mut host, buffer, _) = host_with_selected_buffer(BufferKind::Log, "log");

        let result =
            apply_keyboard_event(&mut host, &key_down(Key::Character("a".into()), Some("a")));

        assert!(!result.changed);
        assert_eq!(host.buffer(buffer).unwrap().text(), "log");
    }

    #[test]
    fn returns_unchanged_without_selected_view() {
        let mut host = InterfaceHost::new(PaneTree::new(PaneNode::pane(PANE)));

        let result =
            apply_keyboard_event(&mut host, &key_down(Key::Character("a".into()), Some("a")));

        assert!(!result.changed);
    }
}
