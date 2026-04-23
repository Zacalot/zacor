use crate::events::{self, AppEvent};
use crate::lua::LuaRuntime;
use crate::ui::{self, Buffer, Line, Minibuffer, MinibufferMode, Window};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::cell::{Ref, RefCell};
use std::io::{self, Stdout};
use std::rc::Rc;
use std::time::Duration;

const TICK_RATE: Duration = Duration::from_millis(100);

pub fn run() -> Result<()> {
    let mut terminal = TerminalGuard::enter()?;
    let mut app = App::new()?;

    loop {
        if app.state().should_quit {
            break;
        }
        terminal.draw(|frame| ui::render::render(frame, &app))?;
        match events::next(TICK_RATE)? {
            AppEvent::Key(key) => app.handle_key(key),
            AppEvent::Tick => {}
        }
    }

    Ok(())
}

pub struct App {
    state: SharedAppState,
    lua: LuaRuntime,
}

pub type SharedAppState = Rc<RefCell<AppState>>;

pub struct AppState {
    pub buffers: Vec<Buffer>,
    pub windows: Vec<Window>,
    pub current_window: usize,
    pub minibuffer: Minibuffer,
    pub should_quit: bool,
    next_buffer_id: u64,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            buffers: vec![Buffer {
                id: 1,
                name: "*scratch*".into(),
                lines: vec![Line {
                    text: "zred: Ctrl-Q or :q to quit".into(),
                    record: None,
                }],
            }],
            windows: vec![Window { id: 1, buffer_id: 1 }],
            current_window: 0,
            minibuffer: Minibuffer::message("Ready"),
            should_quit: false,
            next_buffer_id: 2,
        }
    }

    pub fn create_buffer(&mut self, name: &str) -> u64 {
        let id = self.next_buffer_id;
        self.next_buffer_id += 1;
        self.buffers.push(Buffer {
            id,
            name: name.into(),
            lines: Vec::new(),
        });
        id
    }

    pub fn append_to_buffer(&mut self, buffer_id: u64, text: &str) -> bool {
        let Some(buffer) = self.buffers.iter_mut().find(|buffer| buffer.id == buffer_id) else {
            return false;
        };

        for line in text.lines() {
            buffer.lines.push(Line {
                text: line.to_string(),
                record: None,
            });
        }
        if text.ends_with('\n') {
            buffer.lines.push(Line {
                text: String::new(),
                record: None,
            });
        }
        true
    }

    pub fn set_buffer_contents(&mut self, buffer_id: u64, text: &str) -> bool {
        let Some(buffer) = self.buffers.iter_mut().find(|buffer| buffer.id == buffer_id) else {
            return false;
        };

        buffer.lines.clear();
        for line in text.lines() {
            buffer.lines.push(Line {
                text: line.to_string(),
                record: None,
            });
        }
        if buffer.lines.is_empty() || text.ends_with('\n') {
            buffer.lines.push(Line {
                text: String::new(),
                record: None,
            });
        }
        true
    }

    pub fn focus_buffer(&mut self, buffer_id: u64) -> bool {
        if !self.buffers.iter().any(|buffer| buffer.id == buffer_id) {
            return false;
        }
        if let Some(window) = self.windows.get_mut(self.current_window) {
            window.buffer_id = buffer_id;
            return true;
        }
        false
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.minibuffer = Minibuffer::message(status);
    }

    pub fn current_buffer(&self) -> &Buffer {
        let window = &self.windows[self.current_window];
        self.buffers
            .iter()
            .find(|buffer| buffer.id == window.buffer_id)
            .expect("window buffer should exist")
    }
}

impl App {
    pub fn new() -> Result<Self> {
        let state = Rc::new(RefCell::new(AppState::new()));
        let lua = LuaRuntime::new(state.clone())?;
        Ok(Self { state, lua })
    }

    pub fn state(&self) -> Ref<'_, AppState> {
        self.state.borrow()
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('q') {
            self.state.borrow_mut().should_quit = true;
            return;
        }

        let mode = self.state.borrow().minibuffer.mode;
        match mode {
            MinibufferMode::Command => self.handle_command_key(key),
            MinibufferMode::Message => self.handle_normal_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(':') => {
                self.state.borrow_mut().minibuffer = Minibuffer::command();
            }
            KeyCode::Char('n') => {
                let mut state = self.state.borrow_mut();
                let name = format!("*buffer-{}*", state.next_buffer_id);
                let id = state.create_buffer(&name);
                state.append_to_buffer(id, &format!("Buffer {name}"));
                state.focus_buffer(id);
                state.set_status(format!("Created {name}"));
            }
            _ => {}
        }
    }

    fn handle_command_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.state.borrow_mut().minibuffer = Minibuffer::message("Command cancelled");
            }
            KeyCode::Backspace => {
                self.state.borrow_mut().minibuffer.input.pop();
            }
            KeyCode::Enter => {
                let command = self.state.borrow().minibuffer.input.trim().to_string();
                self.execute_command(&command);
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.borrow_mut().minibuffer.input.push(ch);
            }
            _ => {}
        }
    }

    fn execute_command(&mut self, command: &str) {
        match command.split_once(' ') {
            Some(("eval", script)) => match self.lua.eval(script) {
                Ok(()) => {}
                Err(error) => self
                    .state
                    .borrow_mut()
                    .set_status(format!("Lua error: {error:#}")),
            },
            _ => match command {
            "q" | "quit" => {
                self.state.borrow_mut().should_quit = true;
            }
            "eval" => {
                self.state
                    .borrow_mut()
                    .set_status("Usage: :eval <lua code>");
            }
            "" => {
                self.state.borrow_mut().minibuffer = Minibuffer::message("Ready");
            }
            other => {
                self.state
                    .borrow_mut()
                    .minibuffer = Minibuffer::message(format!("Unknown command: :{other}"));
            }
            },
        }
    }
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;
        Ok(Self { terminal })
    }

    fn draw<F>(&mut self, render: F) -> Result<()>
    where
        F: FnOnce(&mut ratatui::Frame<'_>),
    {
        self.terminal.draw(render)?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen, cursor::Show);
        let _ = self.terminal.show_cursor();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_command_sets_should_quit() {
        let mut app = App::new().unwrap();
        app.execute_command("q");
        assert!(app.state().should_quit);
    }

    #[test]
    fn unknown_command_updates_minibuffer() {
        let mut app = App::new().unwrap();
        app.execute_command("bogus");
        assert_eq!(app.state().minibuffer.input, "Unknown command: :bogus");
        assert_eq!(app.state().minibuffer.mode, MinibufferMode::Message);
    }

    #[test]
    fn new_app_has_single_scratch_buffer() {
        let app = App::new().unwrap();
        assert_eq!(app.state().buffers.len(), 1);
        assert_eq!(app.state().current_buffer().name, "*scratch*");
    }

    #[test]
    fn eval_command_can_mutate_editor_state() {
        let mut app = App::new().unwrap();
        app.execute_command("eval minibuffer.message('from lua')");
        assert_eq!(app.state().minibuffer.input, "from lua");
    }
}
