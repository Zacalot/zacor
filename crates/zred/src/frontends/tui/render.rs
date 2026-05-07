use crate::kernel::{MessageLevel, MinibufferMode, SplitAxis};
use crate::session::{
    SessionJobKindView, SessionJobStatusView, SessionJobView, SessionPaneContentView,
    SessionPaneNode, SessionPaneView, SessionSelectedItemView, SessionTreeNodeView, SessionView,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

pub fn render(frame: &mut Frame<'_>, view: &SessionView) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

    render_pane_node(frame, layout[0], &view.pane_tree);

    let jobs = Paragraph::new(format_jobs_line(view));
    frame.render_widget(jobs, layout[1]);

    let message = match view.messages.last() {
        Some(message) => Paragraph::new(message.text.clone()).style(message_style(message.level)),
        None => Paragraph::new(String::new()),
    };
    frame.render_widget(message, layout[2]);

    let status = Paragraph::new(format_status_line(view))
        .style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(status, layout[3]);
}

fn format_jobs_line(view: &SessionView) -> String {
    if view.jobs.is_empty() {
        return String::new();
    }

    let jobs = view
        .jobs
        .iter()
        .map(format_job_summary)
        .collect::<Vec<_>>()
        .join(" | ");
    format!("jobs: {jobs}")
}

fn format_status_line(view: &SessionView) -> String {
    if matches!(view.minibuffer_mode, MinibufferMode::Message)
        && view.minibuffer_text == "Ready"
        && let Some(selected) = &view.selected_item
    {
        return format_selected_item(selected);
    }

    let prefix = match view.minibuffer_mode {
        MinibufferMode::Command => ":",
        MinibufferMode::Message => "",
    };
    format!("{prefix}{}", view.minibuffer_text)
}

fn format_selected_item(item: &SessionSelectedItemView) -> String {
    match item {
        SessionSelectedItemView::Record { row, value } => format!("record {}: {}", row + 1, value),
        SessionSelectedItemView::TreeNode {
            id,
            label,
            linked_buffer_id,
        } => match linked_buffer_id {
            Some(buffer_id) => format!("tree {}: {} -> buffer {}", id, label, buffer_id),
            None => format!("tree {}: {}", id, label),
        },
    }
}

fn message_style(level: MessageLevel) -> Style {
    match level {
        MessageLevel::Info => Style::default().fg(Color::Blue),
        MessageLevel::Warning => Style::default().fg(Color::Yellow),
        MessageLevel::Error => Style::default().fg(Color::Red),
    }
}

fn format_job_summary(job: &SessionJobView) -> String {
    let kind = match &job.kind {
        SessionJobKindView::Generic => job.name.clone(),
        SessionJobKindView::PackageInvoke {
            package, command, ..
        } => format!("{package} {command}"),
    };
    format!("{}:{} [{}]", job.id, kind, format_job_status(&job.status))
}

fn format_job_status(status: &SessionJobStatusView) -> &str {
    match status {
        SessionJobStatusView::Pending => "pending",
        SessionJobStatusView::Running => "running",
        SessionJobStatusView::Succeeded => "succeeded",
        SessionJobStatusView::Failed(_) => "failed",
        SessionJobStatusView::Cancelled => "cancelled",
    }
}

fn render_pane_node(frame: &mut Frame<'_>, area: Rect, node: &SessionPaneNode) {
    match node {
        SessionPaneNode::Leaf(view) => render_pane_leaf(frame, area, view),
        SessionPaneNode::Split {
            axis,
            ratio_percent,
            first,
            second,
        } => {
            let direction = match axis {
                SplitAxis::Horizontal => Direction::Vertical,
                SplitAxis::Vertical => Direction::Horizontal,
            };
            let layout = Layout::default()
                .direction(direction)
                .constraints([
                    Constraint::Percentage(u16::from(*ratio_percent)),
                    Constraint::Percentage(u16::from(100u8.saturating_sub(*ratio_percent))),
                ])
                .split(area);
            render_pane_node(frame, layout[0], first);
            render_pane_node(frame, layout[1], second);
        }
    }
}

fn render_pane_leaf(frame: &mut Frame<'_>, area: Rect, view: &SessionPaneView) {
    let lines = pane_lines(view)
        .into_iter()
        .map(Line::raw)
        .collect::<Vec<_>>();
    let block = if view.active {
        Block::default()
            .title(format_pane_title(view))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
    } else {
        Block::default()
            .title(format_pane_title(view))
            .borders(Borders::ALL)
    };
    let body = Paragraph::new(lines).block(block);
    frame.render_widget(body, area);
}

fn format_pane_title(view: &SessionPaneView) -> String {
    format!(" {} [pane:{}] ", view.buffer_name, view.pane_id)
}

fn pane_lines(view: &SessionPaneView) -> Vec<String> {
    match &view.content {
        SessionPaneContentView::Text(lines) => lines.clone(),
        SessionPaneContentView::Records(records) => records_lines(view, records),
        SessionPaneContentView::Tree(roots) => {
            let mut lines = Vec::new();
            for root in roots {
                push_tree_lines(&mut lines, view, root, 0);
            }
            lines
        }
        SessionPaneContentView::Terminal { transcript } => {
            if transcript.is_empty() {
                vec!["[terminal buffer]".to_string()]
            } else {
                transcript.clone()
            }
        }
        SessionPaneContentView::Browser { url, title } => {
            let mut lines = Vec::new();
            if let Some(title) = title {
                lines.push(format!("title: {title}"));
            }
            if let Some(url) = url {
                lines.push(format!("url: {url}"));
            }
            if lines.is_empty() {
                lines.push("[browser buffer]".to_string());
            }
            lines
        }
        SessionPaneContentView::Media { source } => {
            vec![
                source
                    .clone()
                    .unwrap_or_else(|| "[media buffer]".to_string()),
            ]
        }
        SessionPaneContentView::Canvas { name } => {
            vec![
                name.clone()
                    .unwrap_or_else(|| "[canvas buffer]".to_string()),
            ]
        }
    }
}

fn records_lines(view: &SessionPaneView, records: &[serde_json::Value]) -> Vec<String> {
    if view.buffer_name == "*jobs*" {
        let lines = records
            .iter()
            .enumerate()
            .map(|(index, record)| format_job_record_line(view, index, record))
            .collect::<Vec<_>>();
        if lines.is_empty() {
            vec!["[no jobs]".to_string()]
        } else {
            lines
        }
    } else {
        let lines = records
            .iter()
            .enumerate()
            .map(|(index, record)| format_generic_record_line(view, index, record))
            .collect::<Vec<_>>();
        if lines.is_empty() {
            vec!["[no records]".to_string()]
        } else {
            lines
        }
    }
}

fn format_generic_record_line(
    view: &SessionPaneView,
    index: usize,
    record: &serde_json::Value,
) -> String {
    let prefix = if is_selected_record_row(view, index) {
        "> "
    } else {
        "  "
    };
    format!("{prefix}{record}")
}

fn format_job_record_line(
    view: &SessionPaneView,
    index: usize,
    record: &serde_json::Value,
) -> String {
    let prefix = if is_selected_job_row(view, index) {
        "> "
    } else {
        "  "
    };

    if let Some(summary) = record.get("summary").and_then(serde_json::Value::as_str) {
        return format!("{prefix}{summary}");
    }

    let id = record
        .get("id")
        .and_then(serde_json::Value::as_u64)
        .map(|id| id.to_string())
        .unwrap_or_else(|| "?".to_string());
    let name = record
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("job");
    let status = record
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    format!("{prefix}{id}: {name} [{status}]")
}

fn is_selected_job_row(view: &SessionPaneView, index: usize) -> bool {
    is_selected_record_row(view, index)
}

fn is_selected_record_row(view: &SessionPaneView, index: usize) -> bool {
    matches!(
        view.selection.as_ref(),
        Some(crate::kernel::Selection::Records(selection)) if selection.rows().first() == Some(&index)
    )
}

fn push_tree_lines(
    lines: &mut Vec<String>,
    view: &SessionPaneView,
    node: &SessionTreeNodeView,
    depth: usize,
) {
    let prefix = if is_selected_tree_node(view, node.id.as_str()) {
        "> "
    } else {
        "  "
    };
    lines.push(format!("{prefix}{}{}", "  ".repeat(depth), node.label));
    for child in &node.children {
        push_tree_lines(lines, view, child, depth + 1);
    }
}

fn is_selected_tree_node(view: &SessionPaneView, node_id: &str) -> bool {
    matches!(
        view.selection.as_ref(),
        Some(crate::kernel::Selection::Tree(selection)) if selection.node_ids().first().map(String::as_str) == Some(node_id)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::{BufferKind, MessageLevel, PanePresentation, Viewport};
    use crate::session::{
        SessionJobKindView, SessionJobStatusView, SessionJobView, SessionMessageView, SessionView,
    };
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::style::Color;

    fn render_view(view: &SessionView, width: u16, height: u16) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");

        terminal
            .draw(|frame| render(frame, view))
            .expect("render should succeed");

        terminal.backend().buffer().clone()
    }

    fn buffer_row(buffer: &ratatui::buffer::Buffer, y: u16, width: u16) -> String {
        (0..width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
    }

    #[test]
    fn render_draws_split_panes_and_status_line() {
        let view = SessionView {
            pane_tree: SessionPaneNode::Split {
                axis: SplitAxis::Vertical,
                ratio_percent: 50,
                first: Box::new(SessionPaneNode::Leaf(SessionPaneView {
                    buffer_name: "*scratch*".to_string(),
                    buffer_id: 1,
                    buffer_kind: BufferKind::Text,
                    pane_id: 1,
                    viewport: Viewport::default(),
                    presentation: PanePresentation::Default,
                    selection: None,
                    content: SessionPaneContentView::Text(vec!["zred".to_string()]),
                    active: false,
                })),
                second: Box::new(SessionPaneNode::Leaf(SessionPaneView {
                    buffer_name: "*scratch*".to_string(),
                    buffer_id: 1,
                    buffer_kind: BufferKind::Text,
                    pane_id: 2,
                    viewport: Viewport::default(),
                    presentation: PanePresentation::Default,
                    selection: None,
                    content: SessionPaneContentView::Text(vec!["zred".to_string()]),
                    active: true,
                })),
            },
            jobs: Vec::new(),
            selected_job: None,
            selected_item: None,
            messages: vec![SessionMessageView {
                level: MessageLevel::Info,
                text: "message".to_string(),
            }],
            minibuffer_mode: MinibufferMode::Command,
            minibuffer_text: "help".to_string(),
        };

        let buffer = render_view(&view, 40, 8);

        assert_eq!(buffer[(0, 0)].symbol(), "┌");
        assert_eq!(buffer[(19, 0)].symbol(), "┐");
        assert_eq!(buffer[(20, 0)].symbol(), "┌");
        assert_eq!(buffer[(39, 0)].symbol(), "┐");
        assert_eq!(buffer[(1, 0)].symbol(), " ");
        assert_eq!(buffer[(2, 0)].symbol(), "*");
        assert_eq!(buffer[(1, 1)].symbol(), "z");
        assert_eq!(buffer[(21, 1)].symbol(), "z");
        assert_eq!(buffer[(0, 6)].symbol(), "m");
        assert_eq!(buffer[(0, 7)].symbol(), ":");
        assert_eq!(buffer[(1, 7)].symbol(), "h");
        assert_eq!(buffer[(2, 7)].symbol(), "e");
        assert_eq!(buffer[(20, 0)].fg, Color::Yellow);
        assert_eq!(buffer[(0, 0)].fg, Color::Reset);
    }

    #[test]
    fn render_draws_latest_message_above_status_line() {
        let view = SessionView {
            pane_tree: SessionPaneNode::Leaf(SessionPaneView {
                buffer_name: "notes".to_string(),
                buffer_id: 1,
                buffer_kind: BufferKind::Text,
                pane_id: 1,
                viewport: Viewport::default(),
                presentation: PanePresentation::Default,
                selection: None,
                content: SessionPaneContentView::Text(vec!["body".to_string()]),
                active: true,
            }),
            jobs: Vec::new(),
            selected_job: None,
            selected_item: None,
            messages: vec![
                SessionMessageView {
                    level: MessageLevel::Info,
                    text: "startup complete".to_string(),
                },
                SessionMessageView {
                    level: MessageLevel::Error,
                    text: "package failed".to_string(),
                },
            ],
            minibuffer_mode: MinibufferMode::Message,
            minibuffer_text: "Ready".to_string(),
        };

        let buffer = render_view(&view, 40, 8);

        assert_eq!(buffer[(0, 6)].symbol(), "p");
        assert_eq!(buffer[(1, 6)].symbol(), "a");
        assert_eq!(buffer[(2, 6)].symbol(), "c");
        assert_eq!(buffer[(0, 6)].fg, Color::Red);
    }

    #[test]
    fn render_draws_jobs_line_above_message_line() {
        let view = SessionView {
            pane_tree: SessionPaneNode::Leaf(SessionPaneView {
                buffer_name: "notes".to_string(),
                buffer_id: 1,
                buffer_kind: BufferKind::Text,
                pane_id: 1,
                viewport: Viewport::default(),
                presentation: PanePresentation::Default,
                selection: None,
                content: SessionPaneContentView::Text(vec!["body".to_string()]),
                active: true,
            }),
            jobs: vec![
                SessionJobView {
                    id: 3,
                    name: "index workspace".to_string(),
                    status: SessionJobStatusView::Running,
                    owner: None,
                    kind: SessionJobKindView::Generic,
                },
                SessionJobView {
                    id: 4,
                    name: "package echo default".to_string(),
                    status: SessionJobStatusView::Succeeded,
                    owner: None,
                    kind: SessionJobKindView::PackageInvoke {
                        package: "echo".to_string(),
                        command: "default".to_string(),
                        output_buffer_id: 2,
                    },
                },
            ],
            selected_job: None,
            selected_item: None,
            messages: vec![SessionMessageView {
                level: MessageLevel::Info,
                text: "ready".to_string(),
            }],
            minibuffer_mode: MinibufferMode::Message,
            minibuffer_text: "Ready".to_string(),
        };

        let buffer = render_view(&view, 80, 8);
        let jobs_line = buffer_row(&buffer, 5, 80);

        assert!(jobs_line.contains("jobs:"));
        assert!(jobs_line.contains("3:index workspace [running]"));
        assert!(jobs_line.contains("4:echo default [succeeded]"));
        assert_eq!(buffer[(0, 6)].symbol(), "r");
    }

    #[test]
    fn render_formats_jobs_buffer_records_as_human_readable_lines() {
        let view = SessionView {
            pane_tree: SessionPaneNode::Leaf(SessionPaneView {
                buffer_name: "*jobs*".to_string(),
                buffer_id: 7,
                buffer_kind: BufferKind::Records,
                pane_id: 1,
                viewport: Viewport::default(),
                presentation: PanePresentation::Default,
                selection: Some(crate::kernel::Selection::Records(
                    crate::kernel::RecordSelection::new(vec![0]),
                )),
                content: SessionPaneContentView::Records(vec![serde_json::json!({
                    "id": 1,
                    "summary": "package echo default [succeeded] echo default -> buffer 2"
                })]),
                active: true,
            }),
            jobs: Vec::new(),
            selected_job: Some(SessionJobView {
                id: 1,
                name: "package echo default".to_string(),
                status: SessionJobStatusView::Succeeded,
                owner: None,
                kind: SessionJobKindView::PackageInvoke {
                    package: "echo".to_string(),
                    command: "default".to_string(),
                    output_buffer_id: 2,
                },
            }),
            selected_item: Some(crate::session::SessionSelectedItemView::Record {
                row: 0,
                value: serde_json::json!({
                    "id": 1,
                    "summary": "package echo default [succeeded] echo default -> buffer 2"
                }),
            }),
            messages: Vec::new(),
            minibuffer_mode: MinibufferMode::Message,
            minibuffer_text: "Ready".to_string(),
        };

        let buffer = render_view(&view, 80, 8);
        let pane_line = buffer_row(&buffer, 1, 80);

        assert!(pane_line.contains("> package echo default [succeeded] echo default -> buffer 2"));
        assert!(!pane_line.contains("\"summary\""));
    }

    #[test]
    fn render_marks_selected_tree_node() {
        let view = SessionView {
            pane_tree: SessionPaneNode::Leaf(SessionPaneView {
                buffer_name: "tree".to_string(),
                buffer_id: 9,
                buffer_kind: BufferKind::Tree,
                pane_id: 1,
                viewport: Viewport::default(),
                presentation: PanePresentation::Default,
                selection: Some(crate::kernel::Selection::Tree(
                    crate::kernel::TreeSelection::new(vec!["child".to_string()]),
                )),
                content: SessionPaneContentView::Tree(vec![SessionTreeNodeView {
                    id: "root".to_string(),
                    label: "Root".to_string(),
                    children: vec![SessionTreeNodeView {
                        id: "child".to_string(),
                        label: "Child".to_string(),
                        children: Vec::new(),
                    }],
                }]),
                active: true,
            }),
            jobs: Vec::new(),
            selected_job: None,
            selected_item: Some(crate::session::SessionSelectedItemView::TreeNode {
                id: "child".to_string(),
                label: "Child".to_string(),
                linked_buffer_id: None,
            }),
            messages: Vec::new(),
            minibuffer_mode: MinibufferMode::Message,
            minibuffer_text: "Ready".to_string(),
        };

        let buffer = render_view(&view, 80, 8);

        assert!(buffer_row(&buffer, 1, 80).contains("  Root"));
        assert!(buffer_row(&buffer, 2, 80).contains(">   Child"));
    }

    #[test]
    fn render_uses_selected_item_summary_in_ready_status() {
        let view = SessionView {
            pane_tree: SessionPaneNode::Leaf(SessionPaneView {
                buffer_name: "tree".to_string(),
                buffer_id: 9,
                buffer_kind: BufferKind::Tree,
                pane_id: 1,
                viewport: Viewport::default(),
                presentation: PanePresentation::Default,
                selection: None,
                content: SessionPaneContentView::Tree(Vec::new()),
                active: true,
            }),
            jobs: Vec::new(),
            selected_job: None,
            selected_item: Some(crate::session::SessionSelectedItemView::TreeNode {
                id: "child".to_string(),
                label: "Child".to_string(),
                linked_buffer_id: Some(7),
            }),
            messages: Vec::new(),
            minibuffer_mode: MinibufferMode::Message,
            minibuffer_text: "Ready".to_string(),
        };

        let buffer = render_view(&view, 80, 8);

        assert!(buffer_row(&buffer, 7, 80).contains("tree child: Child -> buffer 7"));
    }

    #[test]
    fn render_pane_node_respects_split_ratio() {
        let backend = TestBackend::new(40, 6);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        let pane_tree = SessionPaneNode::Split {
            axis: SplitAxis::Vertical,
            ratio_percent: 70,
            first: Box::new(SessionPaneNode::Leaf(SessionPaneView {
                buffer_name: "left".to_string(),
                buffer_id: 1,
                buffer_kind: crate::kernel::BufferKind::Text,
                pane_id: 1,
                viewport: crate::kernel::Viewport::default(),
                presentation: crate::kernel::PanePresentation::Default,
                selection: None,
                content: SessionPaneContentView::Text(vec!["L".to_string()]),
                active: false,
            })),
            second: Box::new(SessionPaneNode::Leaf(SessionPaneView {
                buffer_name: "right".to_string(),
                buffer_id: 2,
                buffer_kind: crate::kernel::BufferKind::Text,
                pane_id: 2,
                viewport: crate::kernel::Viewport::default(),
                presentation: crate::kernel::PanePresentation::Default,
                selection: None,
                content: SessionPaneContentView::Text(vec!["R".to_string()]),
                active: true,
            })),
        };

        terminal
            .draw(|frame| render_pane_node(frame, Rect::new(0, 0, 40, 6), &pane_tree))
            .expect("render should succeed");

        let buffer = terminal.backend().buffer();

        assert_eq!(buffer[(27, 0)].symbol(), "┐");
        assert_eq!(buffer[(28, 0)].symbol(), "┌");
        assert_eq!(buffer[(29, 1)].symbol(), "R");
        assert_eq!(buffer[(28, 0)].fg, Color::Yellow);
        assert_eq!(buffer[(27, 0)].fg, Color::Reset);
    }
}
