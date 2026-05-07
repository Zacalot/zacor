use super::Session;
use crate::kernel::{CommandInvocation, CommandMetadata, CommandResult, KeyChord, KeymapLookup};

impl Session {
    pub fn command_entries(&self) -> Vec<CommandMetadata<'_>> {
        self.workspace.command_entries()
    }

    pub fn command_entry(&self, name: &str) -> Option<CommandMetadata<'_>> {
        self.workspace.command_entry(name)
    }

    pub fn lookup_keymap(&self, sequence: &[KeyChord]) -> KeymapLookup<'_> {
        self.workspace.lookup_keymap(sequence)
    }

    pub fn next_buffer_name(&self) -> String {
        format!("*buffer-{}*", self.buffer_count() + 1)
    }

    pub fn submit_command_input(&mut self) -> CommandResult {
        let command = self.workspace.minibuffer().input().trim().to_string();
        self.workspace.dispatch_command(&command)
    }

    pub fn dispatch_command(&mut self, input: &str) -> CommandResult {
        self.workspace.dispatch_command(input)
    }

    pub fn dispatch_invocation(&mut self, invocation: CommandInvocation) -> CommandResult {
        self.workspace.dispatch_invocation(invocation)
    }
}
