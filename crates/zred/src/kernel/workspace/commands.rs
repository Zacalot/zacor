use super::Workspace;
use crate::kernel::command::{CommandInvocation, CommandMetadata, CommandRegistry, CommandResult};
use crate::kernel::keymap::{KeyChord, KeymapLookup};
use crate::kernel::{JobId, JobStatus};

impl Workspace {
    pub fn commands(&self) -> &CommandRegistry {
        &self.commands
    }

    pub fn command_entries(&self) -> Vec<CommandMetadata<'_>> {
        self.commands.entries().collect()
    }

    pub fn command_entry(&self, name: &str) -> Option<CommandMetadata<'_>> {
        self.commands.entry(name)
    }

    pub fn lookup_keymap(&self, sequence: &[KeyChord]) -> KeymapLookup<'_> {
        self.keymaps.lookup(sequence)
    }

    pub fn dispatch_command(&mut self, input: &str) -> CommandResult {
        let commands = self.commands.clone();
        let request = commands.parse(input);
        commands.dispatch_request(self, request)
    }

    pub fn dispatch_invocation(&mut self, invocation: CommandInvocation) -> CommandResult {
        let commands = self.commands.clone();
        commands.dispatch_invocation(self, invocation)
    }

    pub fn set_job_status(&mut self, job_id: JobId, status: JobStatus) -> bool {
        let Some(job) = self.jobs.get_mut(job_id) else {
            return false;
        };
        job.set_status(status);
        true
    }
}
