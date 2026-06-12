use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BufferId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ViewId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceFrameId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferKind {
    Text,
    Log,
    Output,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Buffer {
    id: BufferId,
    name: String,
    kind: BufferKind,
    revision: u64,
    text: String,
}

impl Buffer {
    fn new(
        id: BufferId,
        kind: BufferKind,
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            revision: 0,
            text: text.into(),
        }
    }

    pub fn id(&self) -> BufferId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> BufferKind {
        self.kind
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.text.lines()
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.revision += 1;
    }

    pub fn append_text(&mut self, text: impl AsRef<str>) {
        self.text.push_str(text.as_ref());
        self.revision += 1;
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BufferStore {
    buffers: BTreeMap<BufferId, Buffer>,
    next_id: u64,
}

impl BufferStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&mut self, kind: BufferKind, name: impl Into<String>) -> BufferId {
        self.create_with_text(kind, name, "")
    }

    pub fn create_with_text(
        &mut self,
        kind: BufferKind,
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> BufferId {
        let id = self.allocate_id();
        let previous = self.buffers.insert(id, Buffer::new(id, kind, name, text));
        debug_assert!(previous.is_none());
        id
    }

    pub fn get(&self, id: BufferId) -> Option<&Buffer> {
        self.buffers.get(&id)
    }

    pub fn get_mut(&mut self, id: BufferId) -> Option<&mut Buffer> {
        self.buffers.get_mut(&id)
    }

    pub fn append_text(&mut self, id: BufferId, text: impl AsRef<str>) -> bool {
        let Some(buffer) = self.buffers.get_mut(&id) else {
            return false;
        };
        buffer.append_text(text);
        true
    }

    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    fn allocate_id(&mut self) -> BufferId {
        let id = BufferId(self.next_id);
        self.next_id += 1;
        id
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ViewCursor {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ViewScroll {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct View {
    id: ViewId,
    buffer: BufferId,
    cursor: ViewCursor,
    scroll: ViewScroll,
}

impl View {
    fn new(id: ViewId, buffer: BufferId) -> Self {
        Self {
            id,
            buffer,
            cursor: ViewCursor::default(),
            scroll: ViewScroll::default(),
        }
    }

    pub fn id(&self) -> ViewId {
        self.id
    }

    pub fn buffer(&self) -> BufferId {
        self.buffer
    }

    pub fn cursor(&self) -> ViewCursor {
        self.cursor
    }

    pub fn set_cursor(&mut self, cursor: ViewCursor) {
        self.cursor = cursor;
    }

    pub fn scroll(&self) -> ViewScroll {
        self.scroll
    }

    pub fn set_scroll(&mut self, scroll: ViewScroll) {
        self.scroll = scroll;
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ViewStore {
    views: BTreeMap<ViewId, View>,
    next_id: u64,
}

impl ViewStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&mut self, buffer: BufferId) -> ViewId {
        let id = self.allocate_id();
        let previous = self.views.insert(id, View::new(id, buffer));
        debug_assert!(previous.is_none());
        id
    }

    pub fn get(&self, id: ViewId) -> Option<&View> {
        self.views.get(&id)
    }

    pub fn get_mut(&mut self, id: ViewId) -> Option<&mut View> {
        self.views.get_mut(&id)
    }

    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    fn allocate_id(&mut self) -> ViewId {
        let id = ViewId(self.next_id);
        self.next_id += 1;
        id
    }
}

/// Retained shell/root-frame identity. Selection state deliberately does not
/// live here: the focus state in `HostRuntime` is the single selection
/// authority, and the active pane / selected view are derived from it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterfaceFrame {
    id: InterfaceFrameId,
}

impl InterfaceFrame {
    pub fn new(id: InterfaceFrameId) -> Self {
        Self { id }
    }

    pub fn id(&self) -> InterfaceFrameId {
        self.id
    }
}

impl Default for InterfaceFrame {
    fn default() -> Self {
        Self::new(InterfaceFrameId(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_store_creates_and_retrieves_buffers() {
        let mut buffers = BufferStore::new();

        let id = buffers.create_with_text(BufferKind::Text, "scratch", "hello");

        let buffer = buffers.get(id).unwrap();
        assert_eq!(buffer.id(), id);
        assert_eq!(buffer.name(), "scratch");
        assert_eq!(buffer.kind(), BufferKind::Text);
        assert_eq!(buffer.text(), "hello");
    }

    #[test]
    fn buffer_append_increments_revision() {
        let mut buffers = BufferStore::new();
        let id = buffers.create(BufferKind::Output, "output");

        assert!(buffers.append_text(id, "hello"));

        let buffer = buffers.get(id).unwrap();
        assert_eq!(buffer.text(), "hello");
        assert_eq!(buffer.revision(), 1);
    }

    #[test]
    fn view_store_creates_views_over_buffers() {
        let mut views = ViewStore::new();
        let buffer = BufferId(7);

        let view = views.create(buffer);

        assert_eq!(views.get(view).unwrap().id(), view);
        assert_eq!(views.get(view).unwrap().buffer(), buffer);
    }

    #[test]
    fn interface_frame_keeps_identity_only() {
        let frame = InterfaceFrame::new(InterfaceFrameId(3));

        assert_eq!(frame.id(), InterfaceFrameId(3));
    }
}
