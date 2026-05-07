use crate::kernel::{KeyChord, KeymapLookup, MinibufferMode};
use crate::session::{Session, SessionLuaRuntime, SessionPackageRuntime, SharedSession};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::cell::Ref;

enum ResolvedKeymapLookup {
    Matched(String),
    Pending,
    NoMatch,
}

pub struct TuiInputController {
    state: SharedSession,
    pending_key_sequence: Vec<KeyChord>,
}

impl TuiInputController {
    pub fn new(state: SharedSession) -> Self {
        Self {
            state,
            pending_key_sequence: Vec::new(),
        }
    }

    pub fn state(&self) -> Ref<'_, Session> {
        Session::borrow(&self.state)
    }

    pub fn request_quit(&mut self) {
        self.state.borrow_mut().request_quit();
    }

    pub fn run_command(
        &mut self,
        command: &str,
        lua_runtime: &mut impl SessionLuaRuntime,
        package_runtime: &mut dyn SessionPackageRuntime,
    ) {
        let result = self.state.borrow_mut().dispatch_command(command);
        Session::apply_command_result_shared(&self.state, result, lua_runtime, package_runtime);
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        lua_runtime: &mut impl SessionLuaRuntime,
        package_runtime: &mut dyn SessionPackageRuntime,
    ) {
        let mode = self.state.borrow().minibuffer().mode();
        match mode {
            MinibufferMode::Command => self.handle_command_key(key, lua_runtime, package_runtime),
            MinibufferMode::Message => self.handle_message_key(key, lua_runtime, package_runtime),
        }
    }

    fn handle_message_key(
        &mut self,
        key: KeyEvent,
        lua_runtime: &mut impl SessionLuaRuntime,
        package_runtime: &mut dyn SessionPackageRuntime,
    ) {
        let Some(chord) = KeyChord::from_event(key) else {
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

    fn run_bound_command(
        &mut self,
        command: &str,
        lua_runtime: &mut impl SessionLuaRuntime,
        package_runtime: &mut dyn SessionPackageRuntime,
    ) {
        match command {
            "app.quit" => self.request_quit(),
            "minibuffer.command.enter" => self.state.borrow_mut().enter_command_mode(),
            "buffer.new.next" => {
                let name = self.state.borrow().next_buffer_name();
                self.run_command(&format!("buffer.new {name}"), lua_runtime, package_runtime);
            }
            other => self.run_command(other, lua_runtime, package_runtime),
        }
    }

    fn handle_command_key(
        &mut self,
        key: KeyEvent,
        lua_runtime: &mut impl SessionLuaRuntime,
        package_runtime: &mut dyn SessionPackageRuntime,
    ) {
        match key.code {
            KeyCode::Esc => {
                self.state.borrow_mut().cancel_command_mode();
            }
            KeyCode::Backspace => {
                self.state.borrow_mut().backspace_command_input();
            }
            KeyCode::Enter => {
                let result = self.state.borrow_mut().submit_command_input();
                Session::apply_command_result_shared(
                    &self.state,
                    result,
                    lua_runtime,
                    package_runtime,
                );
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.borrow_mut().push_command_input(ch);
            }
            _ => {}
        }
    }
}
