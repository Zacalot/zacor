use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Selection {
    Text(TextSelection),
    Records(RecordSelection),
    Tree(TreeSelection),
    Surface(SurfaceSelection),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextSelection {
    ranges: Vec<TextRange>,
}

impl TextSelection {
    #[allow(dead_code)]
    pub fn new(ranges: Vec<TextRange>) -> Self {
        Self { ranges }
    }

    pub fn cursor(position: usize) -> Self {
        Self {
            ranges: vec![TextRange::new(position, position)],
        }
    }

    pub fn ranges(&self) -> &[TextRange] {
        &self.ranges
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextRange {
    anchor: usize,
    head: usize,
}

impl TextRange {
    pub fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    pub fn anchor(&self) -> usize {
        self.anchor
    }

    pub fn head(&self) -> usize {
        self.head
    }

    pub fn is_cursor(&self) -> bool {
        self.anchor == self.head
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecordSelection {
    rows: Vec<usize>,
}

#[allow(dead_code)]
impl RecordSelection {
    pub fn new(rows: Vec<usize>) -> Self {
        Self { rows }
    }

    pub fn rows(&self) -> &[usize] {
        &self.rows
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TreeSelection {
    node_ids: Vec<String>,
}

#[allow(dead_code)]
impl TreeSelection {
    pub fn new(node_ids: Vec<String>) -> Self {
        Self { node_ids }
    }

    pub fn node_ids(&self) -> &[String] {
        &self.node_ids
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SurfaceSelection {
    target: String,
}

impl SurfaceSelection {
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
        }
    }

    #[allow(dead_code)]
    pub fn target(&self) -> &str {
        &self.target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_is_a_degenerate_text_selection() {
        let selection = TextSelection::cursor(7);
        let range = selection.ranges()[0];

        assert_eq!(range.anchor(), 7);
        assert_eq!(range.head(), 7);
        assert!(range.is_cursor());
    }
}
