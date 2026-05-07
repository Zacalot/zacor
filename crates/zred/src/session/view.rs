use super::Session;
use crate::kernel::{Buffer, BufferContent, MessageLevel, MinibufferMode, PaneNode, SplitAxis};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionView {
    pub pane_tree: SessionPaneNode,
    pub message_line: Option<SessionMessageView>,
    pub status_line: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionMessageView {
    pub level: MessageLevel,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionPaneNode {
    Leaf(SessionPaneView),
    Split {
        axis: SplitAxis,
        ratio_percent: u8,
        first: Box<SessionPaneNode>,
        second: Box<SessionPaneNode>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionPaneView {
    pub title: String,
    pub lines: Vec<String>,
    pub active: bool,
}

impl Session {
    pub fn view(&self) -> SessionView {
        let status_prefix = match self.minibuffer().mode() {
            MinibufferMode::Command => ":",
            MinibufferMode::Message => "",
        };

        SessionView {
            pane_tree: self.build_pane_view(self.workspace.pane_tree().root()),
            message_line: self.workspace.messages().entries().last().map(|message| {
                SessionMessageView {
                    level: message.level(),
                    text: message.text().to_string(),
                }
            }),
            status_line: format!("{status_prefix}{}", self.minibuffer().input()),
        }
    }

    fn build_pane_view(&self, node: &PaneNode) -> SessionPaneNode {
        match node {
            PaneNode::Leaf(pane_id) => {
                let pane = self
                    .workspace
                    .pane(*pane_id)
                    .expect("pane tree leaf should reference a live pane");
                let buffer = self
                    .workspace
                    .buffer(pane.buffer_id())
                    .expect("pane should reference a live buffer");
                SessionPaneNode::Leaf(SessionPaneView {
                    title: format!(" {} [pane:{}] ", buffer.name(), pane_id.raw()),
                    lines: buffer_lines(buffer),
                    active: *pane_id == self.workspace.active_pane_id(),
                })
            }
            PaneNode::Split {
                axis,
                first,
                second,
                ratio_percent,
            } => SessionPaneNode::Split {
                axis: *axis,
                ratio_percent: *ratio_percent,
                first: Box::new(self.build_pane_view(first)),
                second: Box::new(self.build_pane_view(second)),
            },
        }
    }
}

fn buffer_lines(buffer: &Buffer) -> Vec<String> {
    match buffer.content() {
        BufferContent::Text(content) => content
            .lines()
            .iter()
            .map(|line| line.text().to_string())
            .collect(),
        BufferContent::Records(content) => content
            .records()
            .iter()
            .map(|record| record.to_string())
            .collect(),
        BufferContent::Tree(content) => content
            .roots()
            .iter()
            .map(|node| node.label().to_string())
            .collect(),
        BufferContent::Terminal(_) => vec!["[terminal buffer]".to_string()],
        BufferContent::Browser(content) => {
            vec![content.url().unwrap_or("[browser buffer]").to_string()]
        }
        BufferContent::Media(content) => {
            vec![content.source().unwrap_or("[media buffer]").to_string()]
        }
        BufferContent::Canvas(content) => {
            vec![content.name().unwrap_or("[canvas buffer]").to_string()]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::SplitAxis;

    #[test]
    fn view_preserves_split_pane_structure() {
        let mut session = Session::new();
        session.dispatch_command("pane.split.vertical");
        session.dispatch_command("pane.split.horizontal");

        let view = session.view();

        match view.pane_tree {
            SessionPaneNode::Split {
                axis: SplitAxis::Vertical,
                ratio_percent: 50,
                first,
                second,
            } => {
                assert!(matches!(*first, SessionPaneNode::Leaf(_)));
                assert!(matches!(
                    *second,
                    SessionPaneNode::Split {
                        axis: SplitAxis::Horizontal,
                        ratio_percent: 50,
                        ..
                    }
                ));
            }
            other => panic!("unexpected pane tree: {other:?}"),
        }
    }

    #[test]
    fn view_exposes_latest_workspace_message() {
        let mut session = Session::new();
        session
            .workspace_mut()
            .messages_mut()
            .warn("package failed");

        let view = session.view();

        assert_eq!(
            view.message_line,
            Some(SessionMessageView {
                level: MessageLevel::Warning,
                text: "package failed".to_string(),
            })
        );
    }
}
