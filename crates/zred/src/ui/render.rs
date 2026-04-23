use crate::app::App;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    let state = app.state();
    let buffer = state.current_buffer();
    let title = format!(
        " {} [window:{}] ",
        buffer.name, state.windows[state.current_window].id
    );
    let lines = buffer
        .lines
        .iter()
        .map(|line| {
            let _has_record = line.record.is_some();
            Line::raw(line.text.clone())
        })
        .collect::<Vec<_>>();
    let body = Paragraph::new(lines).block(Block::default().title(title).borders(Borders::ALL));
    frame.render_widget(body, layout[0]);

    let prefix = match state.minibuffer.mode {
        crate::ui::MinibufferMode::Command => ":",
        crate::ui::MinibufferMode::Message => "",
    };
    let status = Paragraph::new(format!("{prefix}{}", state.minibuffer.input)).style(
        Style::default().add_modifier(Modifier::BOLD),
    );
    frame.render_widget(status, layout[1]);
}
