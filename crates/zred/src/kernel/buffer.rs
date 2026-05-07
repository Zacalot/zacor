use crate::kernel::ids::BufferId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCRATCH_BUFFER_NAME: &str = "*scratch*";
pub const SCRATCH_BUFFER_TEXT: &str = "zred: Ctrl-Q or :q to quit";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BufferKind {
    Text,
    Records,
    Tree,
    Terminal,
    Browser,
    Media,
    Canvas,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Buffer {
    id: BufferId,
    name: String,
    content: BufferContent,
}

impl Buffer {
    pub fn new(id: BufferId, name: impl Into<String>, content: BufferContent) -> Self {
        Self {
            id,
            name: name.into(),
            content,
        }
    }

    pub fn scratch(id: BufferId) -> Self {
        Self::text(id, SCRATCH_BUFFER_NAME, SCRATCH_BUFFER_TEXT)
    }

    pub fn text(id: BufferId, name: impl Into<String>, text: &str) -> Self {
        Self::new(id, name, BufferContent::Text(TextContent::from_text(text)))
    }

    pub fn id(&self) -> BufferId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> BufferKind {
        self.content.kind()
    }

    pub fn content(&self) -> &BufferContent {
        &self.content
    }

    pub fn text_content(&self) -> Option<&TextContent> {
        match &self.content {
            BufferContent::Text(content) => Some(content),
            _ => None,
        }
    }

    pub fn append_text(&mut self, text: &str) -> bool {
        let BufferContent::Text(content) = &mut self.content else {
            return false;
        };

        content.append(text);
        true
    }

    pub fn set_text(&mut self, text: &str) -> bool {
        let BufferContent::Text(content) = &mut self.content else {
            return false;
        };

        content.set(text);
        true
    }

    pub fn push_record(&mut self, record: Value) -> bool {
        let BufferContent::Records(content) = &mut self.content else {
            return false;
        };

        content.push(record);
        true
    }

    pub fn set_records(&mut self, records: Vec<Value>) -> bool {
        let BufferContent::Records(content) = &mut self.content else {
            return false;
        };

        content.set(records);
        true
    }

    pub fn set_browser_title(&mut self, title: &str) -> bool {
        let BufferContent::Browser(content) = &mut self.content else {
            return false;
        };

        content.set_title(title);
        true
    }

    pub fn set_browser_url(&mut self, url: &str) -> bool {
        let BufferContent::Browser(content) = &mut self.content else {
            return false;
        };

        content.set_url(url);
        true
    }

    pub fn set_media_source(&mut self, source: &str) -> bool {
        let BufferContent::Media(content) = &mut self.content else {
            return false;
        };

        content.set_source(source);
        true
    }

    pub fn set_canvas_name(&mut self, name: &str) -> bool {
        let BufferContent::Canvas(content) = &mut self.content else {
            return false;
        };

        content.set_name(name);
        true
    }

    pub fn append_terminal_text(&mut self, text: &str) -> bool {
        let BufferContent::Terminal(content) = &mut self.content else {
            return false;
        };

        content.transcript_mut().append(text);
        true
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BufferContent {
    Text(TextContent),
    Records(RecordsContent),
    Tree(TreeContent),
    Terminal(TerminalContent),
    Browser(BrowserContent),
    Media(MediaContent),
    Canvas(CanvasContent),
}

impl BufferContent {
    pub fn kind(&self) -> BufferKind {
        match self {
            Self::Text(_) => BufferKind::Text,
            Self::Records(_) => BufferKind::Records,
            Self::Tree(_) => BufferKind::Tree,
            Self::Terminal(_) => BufferKind::Terminal,
            Self::Browser(_) => BufferKind::Browser,
            Self::Media(_) => BufferKind::Media,
            Self::Canvas(_) => BufferKind::Canvas,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TextContent {
    lines: Vec<TextLine>,
}

impl TextContent {
    #[allow(dead_code)]
    pub fn new(lines: Vec<TextLine>) -> Self {
        Self { lines }
    }

    pub fn from_text(text: &str) -> Self {
        let mut content = Self::default();
        content.set(text);
        content
    }

    pub fn lines(&self) -> &[TextLine] {
        &self.lines
    }

    pub fn append(&mut self, text: &str) {
        for line in text.lines() {
            self.lines.push(TextLine::new(line));
        }
        if text.ends_with('\n') {
            self.lines.push(TextLine::new(""));
        }
    }

    pub fn set(&mut self, text: &str) {
        self.lines.clear();
        self.append(text);
        if self.lines.is_empty() {
            self.lines.push(TextLine::new(""));
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextLine {
    text: String,
}

impl TextLine {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RecordsContent {
    records: Vec<Value>,
}

impl RecordsContent {
    #[allow(dead_code)]
    pub fn new(records: Vec<Value>) -> Self {
        Self { records }
    }

    pub fn records(&self) -> &[Value] {
        &self.records
    }

    pub fn set(&mut self, records: Vec<Value>) {
        self.records = records;
    }

    #[allow(dead_code)]
    pub fn push(&mut self, record: Value) {
        self.records.push(record);
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TreeContent {
    roots: Vec<TreeNode>,
}

impl TreeContent {
    #[allow(dead_code)]
    pub fn new(roots: Vec<TreeNode>) -> Self {
        Self { roots }
    }

    pub fn roots(&self) -> &[TreeNode] {
        &self.roots
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TreeNode {
    id: String,
    label: String,
    linked_buffer_id: Option<BufferId>,
    children: Vec<TreeNode>,
}

#[allow(dead_code)]
impl TreeNode {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            linked_buffer_id: None,
            children: Vec::new(),
        }
    }

    pub fn with_linked_buffer(
        id: impl Into<String>,
        label: impl Into<String>,
        linked_buffer_id: BufferId,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            linked_buffer_id: Some(linked_buffer_id),
            children: Vec::new(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn linked_buffer_id(&self) -> Option<BufferId> {
        self.linked_buffer_id
    }

    pub fn children(&self) -> &[TreeNode] {
        &self.children
    }

    pub fn push_child(&mut self, child: TreeNode) {
        self.children.push(child);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TerminalContent {
    transcript: TextContent,
}

#[allow(dead_code)]
impl TerminalContent {
    pub fn transcript(&self) -> &TextContent {
        &self.transcript
    }

    pub fn transcript_mut(&mut self) -> &mut TextContent {
        &mut self.transcript
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BrowserContent {
    url: Option<String>,
    title: Option<String>,
}

#[allow(dead_code)]
impl BrowserContent {
    pub fn new(url: Option<String>, title: Option<String>) -> Self {
        Self { url, title }
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub fn set_url(&mut self, url: impl Into<String>) {
        self.url = Some(url.into());
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = Some(title.into());
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MediaContent {
    source: Option<String>,
}

#[allow(dead_code)]
impl MediaContent {
    pub fn new(source: Option<String>) -> Self {
        Self { source }
    }

    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub fn set_source(&mut self, source: impl Into<String>) {
        self.source = Some(source.into());
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CanvasContent {
    name: Option<String>,
}

#[allow(dead_code)]
impl CanvasContent {
    pub fn new(name: Option<String>) -> Self {
        Self { name }
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = Some(name.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratch_buffer_is_text_buffer_with_startup_message() {
        let buffer = Buffer::scratch(BufferId::new(1));

        assert_eq!(buffer.name(), SCRATCH_BUFFER_NAME);
        assert_eq!(buffer.kind(), BufferKind::Text);
        let lines = buffer.text_content().unwrap().lines();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text(), SCRATCH_BUFFER_TEXT);
    }

    #[test]
    fn text_content_keeps_trailing_newline_as_empty_line() {
        let content = TextContent::from_text("one\ntwo\n");
        let lines = content.lines();

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].text(), "one");
        assert_eq!(lines[1].text(), "two");
        assert_eq!(lines[2].text(), "");
    }
}
