use super::{Session, SessionLuaRuntime, SessionResult, SharedSession};
use crate::kernel::{BufferId, CommandData, CommandInvocation, CommandMetadata};

struct RejectNestedLuaRuntime;
struct RejectPackageRuntime;

impl SessionLuaRuntime for RejectNestedLuaRuntime {
    fn eval(&mut self, _script: &str) -> SessionResult<()> {
        Err("Lua command dispatch cannot execute nested eval effects".to_string())
    }
}

impl super::SessionPackageRuntime for RejectPackageRuntime {
    fn invoke_package(
        &mut self,
        _request: &crate::kernel::PackageInvocationRequest,
        _on_event: &mut dyn FnMut(super::PackageRunEvent),
    ) -> SessionResult<super::PackageRunResult> {
        Err("Lua command dispatch cannot execute package effects".to_string())
    }
}

#[derive(Clone)]
pub struct LuaCommandApi {
    state: SharedSession,
}

impl LuaCommandApi {
    pub fn new(state: SharedSession) -> Self {
        Self { state }
    }

    pub fn quit(&self) -> SessionResult<()> {
        self.run_command("quit")
    }

    pub fn run_command(&self, input: &str) -> SessionResult<()> {
        let result = self.state.borrow_mut().dispatch_command(input);
        self.apply_command_result(result)
    }

    pub fn commands(&self) -> Vec<LuaCommandInfo> {
        self.state
            .borrow()
            .command_entries()
            .into_iter()
            .map(LuaCommandInfo::from)
            .collect()
    }

    pub fn command(&self, name: &str) -> Option<LuaCommandInfo> {
        self.state
            .borrow()
            .command_entry(name)
            .map(LuaCommandInfo::from)
    }

    fn apply_command_result(&self, result: crate::kernel::CommandResult) -> SessionResult<()> {
        let mut runtime = RejectNestedLuaRuntime;
        let mut package_runtime = RejectPackageRuntime;
        Session::apply_command_result(&self.state, result, &mut runtime, &mut package_runtime)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LuaCommandInfo {
    pub name: String,
    pub summary: String,
    pub scope: String,
    pub usage: Option<String>,
}

impl<'a> From<CommandMetadata<'a>> for LuaCommandInfo {
    fn from(value: CommandMetadata<'a>) -> Self {
        Self {
            name: value.name().to_string(),
            summary: value.summary().to_string(),
            scope: format!("{:?}", value.scope()).to_lowercase(),
            usage: value.usage().map(str::to_string),
        }
    }
}

#[derive(Clone)]
pub struct LuaBufferApi {
    state: SharedSession,
    command: LuaCommandApi,
}

impl LuaBufferApi {
    pub fn new(state: SharedSession) -> Self {
        Self {
            command: LuaCommandApi::new(state.clone()),
            state,
        }
    }

    pub fn create_buffer(&self, name: &str) -> SessionResult<BufferId> {
        let result = self
            .state
            .borrow_mut()
            .dispatch_invocation(CommandInvocation::BufferNew {
                name: name.to_string(),
            });
        let buffer_id = match result.data() {
            Some(CommandData::BufferCreated { buffer_id }) => *buffer_id,
            Some(CommandData::PackageJobStarted { .. }) | None => {
                return Err("buffer.create did not return a created buffer id".to_string());
            }
        };
        self.command.apply_command_result(result)?;
        Ok(buffer_id)
    }

    pub fn append_to_buffer(&self, buffer_id: BufferId, text: &str) -> SessionResult<()> {
        let result = self
            .state
            .borrow_mut()
            .dispatch_invocation(CommandInvocation::BufferAppend {
                buffer_id,
                text: text.to_string(),
            });
        self.command.apply_command_result(result)
    }

    pub fn set_buffer_contents(&self, buffer_id: BufferId, text: &str) -> SessionResult<()> {
        let result =
            self.state
                .borrow_mut()
                .dispatch_invocation(CommandInvocation::BufferSetContents {
                    buffer_id,
                    text: text.to_string(),
                });
        self.command.apply_command_result(result)
    }

    pub fn focus_buffer(&self, buffer_id: BufferId) -> SessionResult<()> {
        let result = self
            .state
            .borrow_mut()
            .dispatch_invocation(CommandInvocation::BufferFocus { buffer_id });
        self.command.apply_command_result(result)
    }
}

#[derive(Clone)]
pub struct LuaMinibufferApi {
    state: SharedSession,
}

impl LuaMinibufferApi {
    pub fn new(state: SharedSession) -> Self {
        Self { state }
    }

    pub fn set_message(&self, text: impl Into<String>) {
        self.state.borrow_mut().set_status(text);
    }
}
