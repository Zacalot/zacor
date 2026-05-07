use crate::kernel::{MessageLevel, SplitAxis};
use crate::session::{Session, SessionPaneNode, SessionPaneView};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

pub fn render(frame: &mut Frame<'_>, state: &Session) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let view = state.view();
    render_pane_node(frame, layout[0], &view.pane_tree);

    let message = match view.message_line {
        Some(message) => Paragraph::new(message.text).style(message_style(message.level)),
        None => Paragraph::new(String::new()),
    };
    frame.render_widget(message, layout[1]);

    let status =
        Paragraph::new(view.status_line).style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(status, layout[2]);
}

fn message_style(level: MessageLevel) -> Style {
    match level {
        MessageLevel::Info => Style::default().fg(Color::Blue),
        MessageLevel::Warning => Style::default().fg(Color::Yellow),
        MessageLevel::Error => Style::default().fg(Color::Red),
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
    let lines = view
        .lines
        .iter()
        .map(|line| Line::raw(line.clone()))
        .collect::<Vec<_>>();
    let block = if view.active {
        Block::default()
            .title(view.title.clone())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
    } else {
        Block::default()
            .title(view.title.clone())
            .borders(Borders::ALL)
    };
    let body = Paragraph::new(lines).block(block);
    frame.render_widget(body, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::style::Color;

    struct NoopLuaRuntime;
    struct NoopPackageRuntime;

    impl crate::session::SessionLuaRuntime for NoopLuaRuntime {
        fn eval(&mut self, _script: &str) -> crate::session::SessionResult<()> {
            Ok(())
        }
    }

    impl crate::session::SessionPackageRuntime for NoopPackageRuntime {
        fn invoke_package(
            &mut self,
            _request: &crate::kernel::PackageInvocationRequest,
            _on_event: &mut dyn FnMut(crate::session::PackageRunEvent),
        ) -> crate::session::SessionResult<crate::session::PackageRunResult> {
            Err("package runtime unavailable in render test".to_string())
        }
    }

    #[test]
    fn render_draws_split_panes_and_status_line() {
        let backend = TestBackend::new(40, 8);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        let state = Session::shared();
        let mut lua_runtime = NoopLuaRuntime;
        let mut package_runtime = NoopPackageRuntime;

        for command in ["pane.split.vertical", "help"] {
            let result = state.borrow_mut().dispatch_command(command);
            Session::apply_command_result_shared(
                &state,
                result,
                &mut lua_runtime,
                &mut package_runtime,
            );
        }

        terminal
            .draw(|frame| {
                let session = state.borrow();
                render(frame, &session);
            })
            .expect("render should succeed");

        let buffer = terminal.backend().buffer();

        assert_eq!(buffer[(0, 0)].symbol(), "┌");
        assert_eq!(buffer[(19, 0)].symbol(), "┐");
        assert_eq!(buffer[(20, 0)].symbol(), "┌");
        assert_eq!(buffer[(39, 0)].symbol(), "┐");
        assert_eq!(buffer[(1, 0)].symbol(), " ");
        assert_eq!(buffer[(2, 0)].symbol(), "*");
        assert_eq!(buffer[(1, 1)].symbol(), "z");
        assert_eq!(buffer[(21, 1)].symbol(), "z");
        assert_eq!(buffer[(0, 6)].symbol(), " ");
        assert_eq!(buffer[(0, 7)].symbol(), "C");
        assert_eq!(buffer[(1, 7)].symbol(), "o");
        assert_eq!(buffer[(2, 7)].symbol(), "m");
        assert_eq!(buffer[(20, 0)].fg, Color::Yellow);
        assert_eq!(buffer[(0, 0)].fg, Color::Reset);
    }

    #[test]
    fn render_draws_latest_message_above_status_line() {
        let backend = TestBackend::new(40, 8);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        let state = Session::shared();

        state
            .borrow_mut()
            .workspace_mut()
            .messages_mut()
            .error("package failed");

        terminal
            .draw(|frame| {
                let session = state.borrow();
                render(frame, &session);
            })
            .expect("render should succeed");

        let buffer = terminal.backend().buffer();

        assert_eq!(buffer[(0, 6)].symbol(), "p");
        assert_eq!(buffer[(1, 6)].symbol(), "a");
        assert_eq!(buffer[(2, 6)].symbol(), "c");
        assert_eq!(buffer[(0, 6)].fg, Color::Red);
    }

    #[test]
    fn render_pane_node_respects_split_ratio() {
        let backend = TestBackend::new(40, 6);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        let pane_tree = SessionPaneNode::Split {
            axis: SplitAxis::Vertical,
            ratio_percent: 70,
            first: Box::new(SessionPaneNode::Leaf(SessionPaneView {
                title: " left ".to_string(),
                lines: vec!["L".to_string()],
                active: false,
            })),
            second: Box::new(SessionPaneNode::Leaf(SessionPaneView {
                title: " right ".to_string(),
                lines: vec!["R".to_string()],
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
