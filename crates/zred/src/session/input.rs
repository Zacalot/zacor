use super::{Session, SessionLuaRuntime, SessionPackageRuntime, SharedSession};
use crate::kernel::{KeyChord, KeyCodeRepr, KeymapLookup, MinibufferMode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppInputEvent {
    pub chord: Option<KeyChord>,
    pub text_input: Option<char>,
}

impl AppInputEvent {
    pub const fn new(chord: Option<KeyChord>, text_input: Option<char>) -> Self {
        Self { chord, text_input }
    }
}

enum ResolvedKeymapLookup {
    Matched(String),
    Pending,
    NoMatch,
}

pub struct SessionInputController {
    state: SharedSession,
    pending_key_sequence: Vec<KeyChord>,
}

impl SessionInputController {
    pub fn new(state: SharedSession) -> Self {
        Self {
            state,
            pending_key_sequence: Vec::new(),
        }
    }

    pub fn run_command(
        &mut self,
        command: &str,
        lua_runtime: &mut dyn SessionLuaRuntime,
        package_runtime: &mut dyn SessionPackageRuntime,
    ) {
        let result = self.state.borrow_mut().dispatch_command(command);
        Session::apply_command_result_shared(&self.state, result, lua_runtime, package_runtime);
    }

    pub fn handle_input(
        &mut self,
        event: AppInputEvent,
        lua_runtime: &mut dyn SessionLuaRuntime,
        package_runtime: &mut dyn SessionPackageRuntime,
    ) {
        let mode = self.state.borrow().minibuffer().mode();
        match mode {
            MinibufferMode::Command => {
                self.handle_command_input(event, lua_runtime, package_runtime)
            }
            MinibufferMode::Message => {
                self.handle_message_input(event, lua_runtime, package_runtime)
            }
        }
    }

    fn handle_message_input(
        &mut self,
        event: AppInputEvent,
        lua_runtime: &mut dyn SessionLuaRuntime,
        package_runtime: &mut dyn SessionPackageRuntime,
    ) {
        if self.handle_structured_buffer_input(event, lua_runtime, package_runtime) {
            self.pending_key_sequence.clear();
            return;
        }

        if self.handle_jobs_buffer_input(event, lua_runtime, package_runtime) {
            self.pending_key_sequence.clear();
            return;
        }

        let Some(chord) = event.chord else {
            self.pending_key_sequence.clear();
            return;
        };
        let had_pending_sequence = !self.pending_key_sequence.is_empty();
        self.pending_key_sequence.push(chord);

        let lookup = {
            let state = self.state.borrow();
            match state.lookup_keymap(&self.pending_key_sequence) {
                KeymapLookup::Matched(command) => {
                    ResolvedKeymapLookup::Matched(command.to_string())
                }
                KeymapLookup::Pending => ResolvedKeymapLookup::Pending,
                KeymapLookup::NoMatch => ResolvedKeymapLookup::NoMatch,
            }
        };

        match lookup {
            ResolvedKeymapLookup::Pending => {}
            ResolvedKeymapLookup::Matched(command) => {
                self.pending_key_sequence.clear();
                self.run_bound_command(&command, lua_runtime, package_runtime);
            }
            ResolvedKeymapLookup::NoMatch => {
                self.pending_key_sequence.clear();
                if had_pending_sequence {
                    self.state.borrow_mut().set_status("Unknown pane key");
                }
            }
        }
    }

    fn handle_jobs_buffer_input(
        &mut self,
        event: AppInputEvent,
        lua_runtime: &mut dyn SessionLuaRuntime,
        package_runtime: &mut dyn SessionPackageRuntime,
    ) -> bool {
        if !self.pending_key_sequence.is_empty() {
            return false;
        }

        let Some(chord) = event.chord else {
            return false;
        };
        if chord.modifiers().control() {
            return false;
        }

        let is_jobs_buffer = {
            let state = self.state.borrow();
            state.workspace().current_buffer().name() == "*jobs*"
        };
        if !is_jobs_buffer {
            return false;
        }

        let command = match chord.code() {
            KeyCodeRepr::Char('j') => Some("job.next"),
            KeyCodeRepr::Char('k') => Some("job.prev"),
            KeyCodeRepr::Char('d') => Some("job.describe"),
            KeyCodeRepr::Char('c') => Some("job.cancel"),
            KeyCodeRepr::Enter => Some("job.focus-output"),
            _ => None,
        };

        let Some(command) = command else {
            return false;
        };

        self.run_bound_command(command, lua_runtime, package_runtime);
        true
    }

    fn handle_structured_buffer_input(
        &mut self,
        event: AppInputEvent,
        lua_runtime: &mut dyn SessionLuaRuntime,
        package_runtime: &mut dyn SessionPackageRuntime,
    ) -> bool {
        if !self.pending_key_sequence.is_empty() {
            return false;
        }

        let Some(chord) = event.chord else {
            return false;
        };
        if chord.modifiers().control() {
            return false;
        }

        let is_structured_buffer = {
            let state = self.state.borrow();
            let buffer = state.workspace().current_buffer();
            buffer.name() != "*jobs*"
                && matches!(
                    buffer.content(),
                    crate::kernel::BufferContent::Records(_)
                        | crate::kernel::BufferContent::Tree(_)
                )
        };
        if !is_structured_buffer {
            return false;
        }

        let command = match chord.code() {
            KeyCodeRepr::Char('j') => Some("buffer.structured.next"),
            KeyCodeRepr::Char('k') => Some("buffer.structured.prev"),
            KeyCodeRepr::Char('d') => Some("buffer.structured.current"),
            KeyCodeRepr::Enter => Some("buffer.structured.open"),
            _ => None,
        };

        let Some(command) = command else {
            return false;
        };

        self.run_bound_command(command, lua_runtime, package_runtime);
        true
    }

    fn run_bound_command(
        &mut self,
        command: &str,
        lua_runtime: &mut dyn SessionLuaRuntime,
        package_runtime: &mut dyn SessionPackageRuntime,
    ) {
        match command {
            "app.quit" => self.state.borrow_mut().request_quit(),
            "minibuffer.command.enter" => self.state.borrow_mut().enter_command_mode(),
            "buffer.new.next" => {
                let name = self.state.borrow().next_buffer_name();
                self.run_command(&format!("buffer.new {name}"), lua_runtime, package_runtime);
            }
            other => self.run_command(other, lua_runtime, package_runtime),
        }
    }

    fn handle_command_input(
        &mut self,
        event: AppInputEvent,
        lua_runtime: &mut dyn SessionLuaRuntime,
        package_runtime: &mut dyn SessionPackageRuntime,
    ) {
        match event.chord.map(|chord| (chord.code(), chord.modifiers())) {
            Some((KeyCodeRepr::Esc, _)) => {
                self.state.borrow_mut().cancel_command_mode();
            }
            Some((KeyCodeRepr::Backspace, _)) => {
                self.state.borrow_mut().backspace_command_input();
            }
            Some((KeyCodeRepr::Enter, _)) => {
                let result = self.state.borrow_mut().submit_command_input();
                Session::apply_command_result_shared(
                    &self.state,
                    result,
                    lua_runtime,
                    package_runtime,
                );
            }
            Some((KeyCodeRepr::Char(_), modifiers)) if !modifiers.control() => {
                if let Some(ch) = event.text_input {
                    self.state.borrow_mut().push_command_input(ch);
                }
            }
            _ => {}
        }
    }
}
