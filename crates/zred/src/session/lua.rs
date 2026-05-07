use super::{Session, SessionLuaRuntime, SessionResult, SharedSession};
use crate::kernel::{BufferContent, BufferId, CommandData, CommandInvocation, CommandMetadata};
use serde_json::Value;

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

    pub fn new_window(&self) -> SessionResult<()> {
        self.run_command("window.new")
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LuaCurrentBufferInfo {
    pub id: u64,
    pub name: String,
    pub kind: String,
    pub text_line_count: Option<usize>,
    pub record_count: Option<usize>,
    pub browser_url: Option<String>,
    pub browser_title: Option<String>,
    pub media_source: Option<String>,
    pub canvas_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LuaRecordInfo {
    pub row: usize,
    pub value: Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LuaTreeNodeInfo {
    pub id: String,
    pub label: String,
    pub linked_buffer_id: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LuaJobInfo {
    pub id: u64,
    pub name: String,
    pub status: String,
    pub status_message: Option<String>,
    pub owner_kind: Option<String>,
    pub owner_id: Option<u64>,
    pub kind: String,
    pub package: Option<String>,
    pub command: Option<String>,
    pub output_buffer_id: Option<u64>,
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
pub struct LuaJobApi {
    state: SharedSession,
}

impl LuaJobApi {
    pub fn new(state: SharedSession) -> Self {
        Self { state }
    }

    pub fn jobs(&self) -> Vec<LuaJobInfo> {
        self.state
            .borrow()
            .workspace()
            .jobs()
            .entries()
            .map(LuaJobInfo::from)
            .collect()
    }

    pub fn job(&self, id: u64) -> Option<LuaJobInfo> {
        self.state
            .borrow()
            .workspace()
            .jobs()
            .get(crate::kernel::JobId::new(id))
            .map(LuaJobInfo::from)
    }

    pub fn current_job(&self) -> Option<LuaJobInfo> {
        let state = self.state.borrow();
        let job_id = state.workspace().selected_job_id_from_jobs_buffer()?;
        state.workspace().jobs().get(job_id).map(LuaJobInfo::from)
    }

    pub fn open_jobs_buffer(&self) -> SessionResult<BufferId> {
        let result = self
            .state
            .borrow_mut()
            .dispatch_invocation(CommandInvocation::JobOpen);
        let buffer_id = match result.data() {
            Some(CommandData::BufferCreated { buffer_id }) => *buffer_id,
            Some(CommandData::PackageJobStarted { .. }) | None => {
                return Err("job.open did not return a created buffer id".to_string());
            }
        };

        let mut runtime = RejectNestedLuaRuntime;
        let mut package_runtime = RejectPackageRuntime;
        Session::apply_command_result(&self.state, result, &mut runtime, &mut package_runtime)?;
        Ok(buffer_id)
    }

    pub fn focus_output(&self, id: Option<u64>) -> SessionResult<()> {
        let result =
            self.state
                .borrow_mut()
                .dispatch_invocation(CommandInvocation::JobFocusOutput {
                    job_id: id.map(crate::kernel::JobId::new),
                });

        let mut runtime = RejectNestedLuaRuntime;
        let mut package_runtime = RejectPackageRuntime;
        Session::apply_command_result(&self.state, result, &mut runtime, &mut package_runtime)
    }

    pub fn next(&self) -> SessionResult<()> {
        self.command_invocation(CommandInvocation::JobNext)
    }

    pub fn previous(&self) -> SessionResult<()> {
        self.command_invocation(CommandInvocation::JobPrevious)
    }

    fn command_invocation(&self, invocation: CommandInvocation) -> SessionResult<()> {
        let result = self.state.borrow_mut().dispatch_invocation(invocation);
        let mut runtime = RejectNestedLuaRuntime;
        let mut package_runtime = RejectPackageRuntime;
        Session::apply_command_result(&self.state, result, &mut runtime, &mut package_runtime)
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
        self.create_buffer_from_invocation(CommandInvocation::BufferNew {
            name: name.to_string(),
        })
    }

    pub fn open_browser(&self, url: &str) -> SessionResult<BufferId> {
        self.create_buffer_from_invocation(CommandInvocation::BrowserOpen {
            url: url.to_string(),
        })
    }

    pub fn open_media(&self, source: &str) -> SessionResult<BufferId> {
        self.create_buffer_from_invocation(CommandInvocation::MediaOpen {
            source: source.to_string(),
        })
    }

    pub fn open_canvas(&self, name: &str) -> SessionResult<BufferId> {
        self.create_buffer_from_invocation(CommandInvocation::CanvasOpen {
            name: name.to_string(),
        })
    }

    pub fn open_terminal(&self, name: &str) -> SessionResult<BufferId> {
        self.create_buffer_from_invocation(CommandInvocation::TerminalOpen {
            name: name.to_string(),
        })
    }

    pub fn append_terminal(&self, buffer_id: BufferId, text: &str) -> SessionResult<()> {
        let result =
            self.state
                .borrow_mut()
                .dispatch_invocation(CommandInvocation::TerminalAppend {
                    buffer_id: Some(buffer_id),
                    text: text.to_string(),
                });
        self.command.apply_command_result(result)
    }

    pub fn append_current_terminal(&self, text: &str) -> SessionResult<()> {
        let result =
            self.state
                .borrow_mut()
                .dispatch_invocation(CommandInvocation::TerminalAppend {
                    buffer_id: None,
                    text: text.to_string(),
                });
        self.command.apply_command_result(result)
    }

    pub fn current_buffer(&self) -> LuaCurrentBufferInfo {
        let session = self.state.borrow();
        let buffer = session.current_buffer();
        LuaCurrentBufferInfo::from(buffer)
    }

    pub fn select_record_row(&self, row: usize) -> SessionResult<()> {
        if self
            .state
            .borrow_mut()
            .workspace_mut()
            .select_record_row(row)
        {
            Ok(())
        } else {
            Err("current buffer does not contain that record row".to_string())
        }
    }

    pub fn current_record(&self) -> Option<LuaRecordInfo> {
        let state = self.state.borrow();
        let row = state.workspace().selected_record_row().unwrap_or(0);
        let value = state.workspace().current_record()?.clone();
        Some(LuaRecordInfo { row, value })
    }

    pub fn next_record(&self) -> SessionResult<()> {
        self.next_structured_item()
    }

    pub fn open_record(&self) -> SessionResult<()> {
        self.open_structured_item()
    }

    pub fn previous_record(&self) -> SessionResult<()> {
        self.previous_structured_item()
    }

    pub fn current_tree_node(&self) -> Option<LuaTreeNodeInfo> {
        let state = self.state.borrow();
        let node = state.workspace().current_tree_node()?;
        Some(LuaTreeNodeInfo {
            id: node.id().to_string(),
            label: node.label().to_string(),
            linked_buffer_id: node.linked_buffer_id().map(|id| id.raw()),
        })
    }

    pub fn open_tree_node(&self) -> SessionResult<()> {
        self.open_structured_item()
    }

    pub fn next_tree_node(&self) -> SessionResult<()> {
        self.next_structured_item()
    }

    pub fn previous_tree_node(&self) -> SessionResult<()> {
        self.previous_structured_item()
    }

    pub fn current_structured_item(&self) -> Option<Value> {
        let state = self.state.borrow();
        match state.workspace().current_buffer().content() {
            BufferContent::Records(_) => self.current_record().map(|record| {
                serde_json::json!({
                    "kind": "record",
                    "row": record.row + 1,
                    "value": record.value,
                })
            }),
            BufferContent::Tree(_) => self.current_tree_node().map(|node| {
                serde_json::json!({
                    "kind": "tree_node",
                    "id": node.id,
                    "label": node.label,
                    "linked_buffer_id": node.linked_buffer_id,
                })
            }),
            _ => None,
        }
    }

    pub fn next_structured_item(&self) -> SessionResult<()> {
        let result = self
            .state
            .borrow_mut()
            .dispatch_invocation(CommandInvocation::BufferStructuredNext);
        self.command.apply_command_result(result)
    }

    pub fn previous_structured_item(&self) -> SessionResult<()> {
        let result = self
            .state
            .borrow_mut()
            .dispatch_invocation(CommandInvocation::BufferStructuredPrevious);
        self.command.apply_command_result(result)
    }

    pub fn open_structured_item(&self) -> SessionResult<()> {
        let result = self
            .state
            .borrow_mut()
            .dispatch_invocation(CommandInvocation::BufferStructuredOpen);
        self.command.apply_command_result(result)
    }

    pub fn save_workspace(&self, path: &str) -> SessionResult<()> {
        self.state
            .borrow_mut()
            .workspace_mut()
            .save_snapshot_file(path)
            .map_err(|error| error.to_string())
    }

    pub fn load_workspace(&self, path: &str) -> SessionResult<()> {
        let workspace = crate::kernel::Workspace::load_snapshot_file(path)
            .map_err(|error| error.to_string())?;
        self.state.borrow_mut().replace_workspace(workspace);
        Ok(())
    }

    pub fn set_browser_title(&self, buffer_id: BufferId, title: &str) -> SessionResult<()> {
        let result =
            self.state
                .borrow_mut()
                .dispatch_invocation(CommandInvocation::BrowserSetTitle {
                    buffer_id: Some(buffer_id),
                    title: title.to_string(),
                });
        self.command.apply_command_result(result)
    }

    pub fn set_current_browser_title(&self, title: &str) -> SessionResult<()> {
        let result =
            self.state
                .borrow_mut()
                .dispatch_invocation(CommandInvocation::BrowserSetTitle {
                    buffer_id: None,
                    title: title.to_string(),
                });
        self.command.apply_command_result(result)
    }

    pub fn set_browser_url(&self, buffer_id: BufferId, url: &str) -> SessionResult<()> {
        let result =
            self.state
                .borrow_mut()
                .dispatch_invocation(CommandInvocation::BrowserSetUrl {
                    buffer_id: Some(buffer_id),
                    url: url.to_string(),
                });
        self.command.apply_command_result(result)
    }

    pub fn set_current_browser_url(&self, url: &str) -> SessionResult<()> {
        let result =
            self.state
                .borrow_mut()
                .dispatch_invocation(CommandInvocation::BrowserSetUrl {
                    buffer_id: None,
                    url: url.to_string(),
                });
        self.command.apply_command_result(result)
    }

    pub fn set_media_source(&self, buffer_id: BufferId, source: &str) -> SessionResult<()> {
        let result =
            self.state
                .borrow_mut()
                .dispatch_invocation(CommandInvocation::MediaSetSource {
                    buffer_id: Some(buffer_id),
                    source: source.to_string(),
                });
        self.command.apply_command_result(result)
    }

    pub fn set_current_media_source(&self, source: &str) -> SessionResult<()> {
        let result =
            self.state
                .borrow_mut()
                .dispatch_invocation(CommandInvocation::MediaSetSource {
                    buffer_id: None,
                    source: source.to_string(),
                });
        self.command.apply_command_result(result)
    }

    pub fn set_canvas_name(&self, buffer_id: BufferId, name: &str) -> SessionResult<()> {
        let result =
            self.state
                .borrow_mut()
                .dispatch_invocation(CommandInvocation::CanvasSetName {
                    buffer_id: Some(buffer_id),
                    name: name.to_string(),
                });
        self.command.apply_command_result(result)
    }

    pub fn set_current_canvas_name(&self, name: &str) -> SessionResult<()> {
        let result =
            self.state
                .borrow_mut()
                .dispatch_invocation(CommandInvocation::CanvasSetName {
                    buffer_id: None,
                    name: name.to_string(),
                });
        self.command.apply_command_result(result)
    }

    fn create_buffer_from_invocation(
        &self,
        invocation: CommandInvocation,
    ) -> SessionResult<BufferId> {
        let result = self.state.borrow_mut().dispatch_invocation(invocation);
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

impl From<&crate::kernel::Buffer> for LuaCurrentBufferInfo {
    fn from(buffer: &crate::kernel::Buffer) -> Self {
        let mut info = Self {
            id: buffer.id().raw(),
            name: buffer.name().to_string(),
            kind: format!("{:?}", buffer.kind()).to_lowercase(),
            text_line_count: None,
            record_count: None,
            browser_url: None,
            browser_title: None,
            media_source: None,
            canvas_name: None,
        };

        match buffer.content() {
            BufferContent::Text(content) => {
                info.text_line_count = Some(content.lines().len());
            }
            BufferContent::Records(content) => {
                info.record_count = Some(content.records().len());
            }
            BufferContent::Browser(content) => {
                info.browser_url = content.url().map(str::to_string);
                info.browser_title = content.title().map(str::to_string);
            }
            BufferContent::Media(content) => {
                info.media_source = content.source().map(str::to_string);
            }
            BufferContent::Canvas(content) => {
                info.canvas_name = content.name().map(str::to_string);
            }
            BufferContent::Tree(_) | BufferContent::Terminal(_) => {}
        }

        info
    }
}

impl From<&crate::kernel::Job> for LuaJobInfo {
    fn from(job: &crate::kernel::Job) -> Self {
        let (status, status_message) = match job.status() {
            crate::kernel::JobStatus::Pending => ("pending".to_string(), None),
            crate::kernel::JobStatus::Running => ("running".to_string(), None),
            crate::kernel::JobStatus::Succeeded => ("succeeded".to_string(), None),
            crate::kernel::JobStatus::Failed(message) => {
                ("failed".to_string(), Some(message.clone()))
            }
            crate::kernel::JobStatus::Cancelled => ("cancelled".to_string(), None),
        };
        let (owner_kind, owner_id) = match job.owner() {
            Some(crate::kernel::JobOwner::Workspace(id)) => {
                (Some("workspace".to_string()), Some(id.raw()))
            }
            Some(crate::kernel::JobOwner::Buffer(id)) => {
                (Some("buffer".to_string()), Some(id.raw()))
            }
            Some(crate::kernel::JobOwner::Pane(id)) => (Some("pane".to_string()), Some(id.raw())),
            None => (None, None),
        };
        let (kind, package, command, output_buffer_id) = match job.kind() {
            crate::kernel::JobKind::Generic => ("generic".to_string(), None, None, None),
            crate::kernel::JobKind::PackageInvoke {
                package,
                command,
                output_buffer_id,
            } => (
                "package_invoke".to_string(),
                Some(package.clone()),
                Some(command.clone()),
                Some(output_buffer_id.raw()),
            ),
        };

        Self {
            id: job.id().raw(),
            name: job.name().to_string(),
            status,
            status_message,
            owner_kind,
            owner_id,
            kind,
            package,
            command,
            output_buffer_id,
        }
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
