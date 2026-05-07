use super::Session;
use crate::kernel::Minibuffer;

impl Session {
    pub fn enter_command_mode(&mut self) {
        *self.workspace.minibuffer_mut() = Minibuffer::command();
    }

    pub fn cancel_command_mode(&mut self) {
        *self.workspace.minibuffer_mut() = Minibuffer::message("Command cancelled");
    }

    pub fn backspace_command_input(&mut self) {
        self.workspace.minibuffer_mut().input_mut().pop();
    }

    pub fn push_command_input(&mut self, ch: char) {
        self.workspace.minibuffer_mut().input_mut().push(ch);
    }
}
