mod buffers;
mod commands;
mod snapshot;

use crate::kernel::buffer::Buffer;
use crate::kernel::capability::{Capability, CapabilityDomain, CapabilityRegistry};
use crate::kernel::command::{Command, CommandRegistry, CommandScope, CommandSpec};
use crate::kernel::ids::{BufferId, IdAllocator, JobId, PaneId, WorkspaceId};
use crate::kernel::job::{Job, JobOwner, JobRegistry};
use crate::kernel::keymap::{KeyChord, KeyCodeRepr, KeyModifiersRepr, KeymapRegistry};
use crate::kernel::messages::MessageLog;
use crate::kernel::minibuffer::Minibuffer;
use crate::kernel::pane::Pane;
use crate::kernel::pane_tree::PaneTree;
use std::collections::BTreeMap;

pub use snapshot::{BufferSnapshot, PaneSnapshot, WorkspaceRestoreError, WorkspaceSnapshot};

#[derive(Clone, Debug)]
pub struct Workspace {
    id: WorkspaceId,
    buffers: BTreeMap<BufferId, Buffer>,
    panes: BTreeMap<PaneId, Pane>,
    pane_tree: PaneTree,
    active_pane: PaneId,
    minibuffer: Minibuffer,
    #[allow(dead_code)]
    messages: MessageLog,
    commands: CommandRegistry,
    keymaps: KeymapRegistry,
    jobs: JobRegistry,
    capabilities: CapabilityRegistry,
    ids: IdAllocator,
}

impl Workspace {
    pub fn new() -> Self {
        Self::with_id(WorkspaceId::new(1))
    }

    pub fn with_id(id: WorkspaceId) -> Self {
        let mut ids = IdAllocator::new();
        let scratch_id = ids.next_buffer_id();
        let pane_id = ids.next_pane_id();

        let mut buffers = BTreeMap::new();
        buffers.insert(scratch_id, Buffer::scratch(scratch_id));

        let mut panes = BTreeMap::new();
        panes.insert(pane_id, Pane::new(pane_id, scratch_id));

        Self {
            id,
            buffers,
            panes,
            pane_tree: PaneTree::single(pane_id),
            active_pane: pane_id,
            minibuffer: Minibuffer::message("Ready"),
            messages: MessageLog::new(),
            commands: default_commands(),
            keymaps: default_keymaps(),
            jobs: JobRegistry::new(),
            capabilities: default_capabilities(),
            ids,
        }
    }

    pub fn id(&self) -> WorkspaceId {
        self.id
    }

    pub fn set_id(&mut self, id: WorkspaceId) {
        self.id = id;
    }

    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    #[allow(dead_code)]
    pub fn buffers(&self) -> impl Iterator<Item = &Buffer> {
        self.buffers.values()
    }

    pub fn buffer(&self, id: BufferId) -> Option<&Buffer> {
        self.buffers.get(&id)
    }

    #[allow(dead_code)]
    pub fn buffer_mut(&mut self, id: BufferId) -> Option<&mut Buffer> {
        self.buffers.get_mut(&id)
    }

    #[allow(dead_code)]
    pub fn panes(&self) -> impl Iterator<Item = &Pane> {
        self.panes.values()
    }

    pub fn pane(&self, id: PaneId) -> Option<&Pane> {
        self.panes.get(&id)
    }

    #[allow(dead_code)]
    pub fn pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        self.panes.get_mut(&id)
    }

    pub fn active_pane_id(&self) -> PaneId {
        self.active_pane
    }

    pub fn active_pane(&self) -> &Pane {
        self.panes
            .get(&self.active_pane)
            .expect("active pane should exist")
    }

    pub fn current_buffer(&self) -> &Buffer {
        self.buffers
            .get(&self.active_pane().buffer_id())
            .expect("active pane buffer should exist")
    }

    pub fn pane_tree(&self) -> &PaneTree {
        &self.pane_tree
    }

    pub fn minibuffer(&self) -> &Minibuffer {
        &self.minibuffer
    }

    pub fn minibuffer_mut(&mut self) -> &mut Minibuffer {
        &mut self.minibuffer
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.minibuffer = Minibuffer::message(status);
    }

    #[allow(dead_code)]
    pub fn messages(&self) -> &MessageLog {
        &self.messages
    }

    #[allow(dead_code)]
    pub fn messages_mut(&mut self) -> &mut MessageLog {
        &mut self.messages
    }

    pub fn jobs(&self) -> &JobRegistry {
        &self.jobs
    }

    #[allow(dead_code)]
    pub fn jobs_mut(&mut self) -> &mut JobRegistry {
        &mut self.jobs
    }

    pub fn create_job(&mut self, name: &str, owner: Option<JobOwner>) -> JobId {
        let id = self.ids.next_job_id();
        self.jobs.insert(Job::new(id, name, owner));
        self.refresh_jobs_buffer_if_present();
        id
    }

    pub fn create_job_with_kind(
        &mut self,
        name: &str,
        owner: Option<JobOwner>,
        kind: crate::kernel::JobKind,
    ) -> JobId {
        let id = self.ids.next_job_id();
        self.jobs.insert(Job::with_kind(id, name, owner, kind));
        self.refresh_jobs_buffer_if_present();
        id
    }

    pub fn capabilities(&self) -> &CapabilityRegistry {
        &self.capabilities
    }

    #[allow(dead_code)]
    pub fn capabilities_mut(&mut self) -> &mut CapabilityRegistry {
        &mut self.capabilities
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

fn default_commands() -> CommandRegistry {
    let mut commands = CommandRegistry::new();
    commands.register(Command::with_spec(
        "help",
        "Show command help",
        CommandScope::Global,
        CommandSpec::help(),
    ));
    commands.register(Command::with_spec(
        "quit",
        "Quit zred",
        CommandScope::Global,
        CommandSpec::quit(),
    ));
    commands.register(Command::with_spec(
        "q",
        "Quit zred",
        CommandScope::Global,
        CommandSpec::quit(),
    ));
    commands.register(Command::with_spec(
        "eval",
        "Evaluate Lua code",
        CommandScope::Workspace,
        CommandSpec::eval_lua(),
    ));
    commands.register(Command::with_spec(
        "window.new",
        "Open a new native window",
        CommandScope::Global,
        CommandSpec::window_new(),
    ));
    commands.register(Command::with_spec(
        "workspace.load",
        "Load workspace from a snapshot file",
        CommandScope::Workspace,
        CommandSpec::workspace_load(),
    ));
    commands.register(Command::with_spec(
        "workspace.save",
        "Save workspace to a snapshot file",
        CommandScope::Workspace,
        CommandSpec::workspace_save(),
    ));
    commands.register(Command::with_spec(
        "job.cancel",
        "Cancel a job",
        CommandScope::Workspace,
        CommandSpec::job_cancel(),
    ));
    commands.register(Command::with_spec(
        "job.describe",
        "Describe a job",
        CommandScope::Workspace,
        CommandSpec::job_describe(),
    ));
    commands.register(Command::with_spec(
        "job.focus-output",
        "Focus the output buffer for a job",
        CommandScope::Workspace,
        CommandSpec::job_focus_output(),
    ));
    commands.register(Command::with_spec(
        "job.list",
        "List jobs",
        CommandScope::Workspace,
        CommandSpec::job_list(),
    ));
    commands.register(Command::with_spec(
        "job.next",
        "Select the next job row",
        CommandScope::Workspace,
        CommandSpec::job_next(),
    ));
    commands.register(Command::with_spec(
        "job.prev",
        "Select the previous job row",
        CommandScope::Workspace,
        CommandSpec::job_previous(),
    ));
    commands.register(Command::with_spec(
        "job.open",
        "Open a jobs buffer",
        CommandScope::Workspace,
        CommandSpec::job_open(),
    ));
    commands.register(Command::with_spec(
        "buffer.structured.current",
        "Describe the selected structured item",
        CommandScope::Workspace,
        CommandSpec::buffer_structured_current(),
    ));
    commands.register(Command::with_spec(
        "buffer.structured.open",
        "Open the linked target for the selected structured item",
        CommandScope::Workspace,
        CommandSpec::buffer_structured_open(),
    ));
    commands.register(Command::with_spec(
        "buffer.structured.next",
        "Select the next structured item",
        CommandScope::Workspace,
        CommandSpec::buffer_structured_next(),
    ));
    commands.register(Command::with_spec(
        "buffer.structured.prev",
        "Select the previous structured item",
        CommandScope::Workspace,
        CommandSpec::buffer_structured_previous(),
    ));
    commands.register(Command::with_spec(
        "buffer.record.current",
        "Describe the selected record row",
        CommandScope::Workspace,
        CommandSpec::buffer_record_current(),
    ));
    commands.register(Command::with_spec(
        "buffer.record.open",
        "Open the linked buffer for the selected record",
        CommandScope::Workspace,
        CommandSpec::buffer_record_open(),
    ));
    commands.register(Command::with_spec(
        "buffer.record.next",
        "Select the next record row",
        CommandScope::Workspace,
        CommandSpec::buffer_record_next(),
    ));
    commands.register(Command::with_spec(
        "buffer.record.prev",
        "Select the previous record row",
        CommandScope::Workspace,
        CommandSpec::buffer_record_previous(),
    ));
    commands.register(Command::with_spec(
        "buffer.tree.current",
        "Describe the selected tree node",
        CommandScope::Workspace,
        CommandSpec::buffer_tree_current(),
    ));
    commands.register(Command::with_spec(
        "buffer.tree.open",
        "Open the linked buffer for the selected tree node",
        CommandScope::Workspace,
        CommandSpec::buffer_tree_open(),
    ));
    commands.register(Command::with_spec(
        "buffer.tree.next",
        "Select the next tree node",
        CommandScope::Workspace,
        CommandSpec::buffer_tree_next(),
    ));
    commands.register(Command::with_spec(
        "buffer.tree.prev",
        "Select the previous tree node",
        CommandScope::Workspace,
        CommandSpec::buffer_tree_previous(),
    ));
    commands.register(Command::with_spec(
        "buffer.new",
        "Create a text buffer",
        CommandScope::Workspace,
        CommandSpec::buffer_new(),
    ));
    commands.register(Command::with_spec(
        "buffer.describe",
        "Describe the active buffer",
        CommandScope::Workspace,
        CommandSpec::buffer_describe(),
    ));
    commands.register(Command::with_spec(
        "terminal.open",
        "Open a terminal buffer",
        CommandScope::Workspace,
        CommandSpec::terminal_open(),
    ));
    commands.register(Command::with_spec(
        "terminal.append",
        "Append transcript text to a terminal buffer",
        CommandScope::Workspace,
        CommandSpec::terminal_append(),
    ));
    commands.register(Command::with_spec(
        "browser.open",
        "Open a browser buffer",
        CommandScope::Workspace,
        CommandSpec::browser_open(),
    ));
    commands.register(Command::with_spec(
        "browser.url.set",
        "Set a browser buffer url",
        CommandScope::Workspace,
        CommandSpec::browser_set_url(),
    ));
    commands.register(Command::with_spec(
        "browser.title.set",
        "Set a browser buffer title",
        CommandScope::Workspace,
        CommandSpec::browser_set_title(),
    ));
    commands.register(Command::with_spec(
        "media.open",
        "Open a media buffer",
        CommandScope::Workspace,
        CommandSpec::media_open(),
    ));
    commands.register(Command::with_spec(
        "media.source.set",
        "Set a media buffer source",
        CommandScope::Workspace,
        CommandSpec::media_set_source(),
    ));
    commands.register(Command::with_spec(
        "canvas.open",
        "Open a canvas buffer",
        CommandScope::Workspace,
        CommandSpec::canvas_open(),
    ));
    commands.register(Command::with_spec(
        "canvas.name.set",
        "Set a canvas buffer name",
        CommandScope::Workspace,
        CommandSpec::canvas_set_name(),
    ));
    commands.register(Command::with_spec(
        "package.run",
        "Run a Zacor package command",
        CommandScope::Workspace,
        CommandSpec::package_run(),
    ));
    commands.register(Command::with_spec(
        "pane.split.horizontal",
        "Split the active pane horizontally",
        CommandScope::Workspace,
        CommandSpec::split_pane_horizontal(),
    ));
    commands.register(Command::with_spec(
        "pane.split.vertical",
        "Split the active pane vertically",
        CommandScope::Workspace,
        CommandSpec::split_pane_vertical(),
    ));
    commands.register(Command::with_spec(
        "pane.next",
        "Focus the next pane",
        CommandScope::Workspace,
        CommandSpec::focus_next_pane(),
    ));
    commands.register(Command::with_spec(
        "pane.prev",
        "Focus the previous pane",
        CommandScope::Workspace,
        CommandSpec::focus_previous_pane(),
    ));
    commands.register(Command::with_spec(
        "pane.left",
        "Focus the pane to the left",
        CommandScope::Workspace,
        CommandSpec::focus_pane_left(),
    ));
    commands.register(Command::with_spec(
        "pane.right",
        "Focus the pane to the right",
        CommandScope::Workspace,
        CommandSpec::focus_pane_right(),
    ));
    commands.register(Command::with_spec(
        "pane.up",
        "Focus the pane above",
        CommandScope::Workspace,
        CommandSpec::focus_pane_up(),
    ));
    commands.register(Command::with_spec(
        "pane.down",
        "Focus the pane below",
        CommandScope::Workspace,
        CommandSpec::focus_pane_down(),
    ));
    commands.register(Command::with_spec(
        "pane.resize.left",
        "Grow the active pane to the left",
        CommandScope::Workspace,
        CommandSpec::resize_pane_left(),
    ));
    commands.register(Command::with_spec(
        "pane.resize.right",
        "Grow the active pane to the right",
        CommandScope::Workspace,
        CommandSpec::resize_pane_right(),
    ));
    commands.register(Command::with_spec(
        "pane.resize.up",
        "Grow the active pane upward",
        CommandScope::Workspace,
        CommandSpec::resize_pane_up(),
    ));
    commands.register(Command::with_spec(
        "pane.resize.down",
        "Grow the active pane downward",
        CommandScope::Workspace,
        CommandSpec::resize_pane_down(),
    ));
    commands
}

fn default_keymaps() -> KeymapRegistry {
    let mut keymaps = KeymapRegistry::new();
    keymaps.bind(
        vec![KeyChord::new(
            KeyCodeRepr::Char(':'),
            KeyModifiersRepr::NONE,
        )],
        "minibuffer.command.enter",
    );
    keymaps.bind(
        vec![KeyChord::new(
            KeyCodeRepr::Char('n'),
            KeyModifiersRepr::NONE,
        )],
        "buffer.new.next",
    );
    keymaps.bind(
        vec![KeyChord::new(
            KeyCodeRepr::Char('q'),
            KeyModifiersRepr::CONTROL,
        )],
        "app.quit",
    );
    bind_ctrl_w_command(&mut keymaps, 'h', "pane.left");
    bind_ctrl_w_command(&mut keymaps, 'j', "pane.down");
    bind_ctrl_w_command(&mut keymaps, 'k', "pane.up");
    bind_ctrl_w_command(&mut keymaps, 'l', "pane.right");
    bind_ctrl_w_command(&mut keymaps, 'H', "pane.resize.left");
    bind_ctrl_w_command(&mut keymaps, 'J', "pane.resize.down");
    bind_ctrl_w_command(&mut keymaps, 'K', "pane.resize.up");
    bind_ctrl_w_command(&mut keymaps, 'L', "pane.resize.right");
    bind_ctrl_w_command(&mut keymaps, 'n', "pane.next");
    bind_ctrl_w_command(&mut keymaps, 'p', "pane.prev");
    bind_ctrl_w_command(&mut keymaps, 's', "pane.split.horizontal");
    bind_ctrl_w_command(&mut keymaps, 'v', "pane.split.vertical");
    keymaps
}

fn bind_ctrl_w_command(keymaps: &mut KeymapRegistry, key: char, command: &str) {
    for modifiers in [KeyModifiersRepr::NONE, KeyModifiersRepr::CONTROL] {
        keymaps.bind(
            vec![
                KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
                KeyChord::new(KeyCodeRepr::Char(key), modifiers),
            ],
            command,
        );
    }
}

fn default_capabilities() -> CapabilityRegistry {
    let mut capabilities = CapabilityRegistry::new();
    for domain in [
        CapabilityDomain::Buffer,
        CapabilityDomain::Window,
        CapabilityDomain::Keymap,
        CapabilityDomain::Minibuffer,
        CapabilityDomain::Workspace,
        CapabilityDomain::Messages,
    ] {
        capabilities.grant(Capability::new(domain, "*"));
    }
    capabilities
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::buffer::{
        BrowserContent, BufferContent, BufferKind, CanvasContent, MediaContent, RecordsContent,
        SCRATCH_BUFFER_NAME, SCRATCH_BUFFER_TEXT, TerminalContent, TextContent, TreeContent,
        TreeNode,
    };
    use crate::kernel::selection::{Selection, SurfaceSelection};
    use crate::kernel::{
        JobKind, JobOwner, JobStatus, KeyChord, KeyCodeRepr, KeyModifiersRepr, KeymapLookup,
    };
    use crate::kernel::{PanePresentation, SplitAxis, Viewport};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn new_workspace_starts_with_scratch_buffer_and_single_pane() {
        let workspace = Workspace::new();

        assert_eq!(workspace.buffer_count(), 1);
        assert_eq!(workspace.pane_count(), 1);
        assert_eq!(workspace.current_buffer().name(), SCRATCH_BUFFER_NAME);
        assert_eq!(workspace.current_buffer().kind(), BufferKind::Text);
        assert_eq!(
            workspace.current_buffer().text_content().unwrap().lines()[0].text(),
            SCRATCH_BUFFER_TEXT
        );
    }

    #[test]
    fn unknown_buffer_ops_fail_softly() {
        let mut workspace = Workspace::new();
        let missing = BufferId::new(999);

        assert!(!workspace.append_to_buffer(missing, "text"));
        assert!(!workspace.set_buffer_contents(missing, "text"));
        assert!(!workspace.focus_buffer(missing));
    }

    #[test]
    fn can_create_focus_and_mutate_text_buffer() {
        let mut workspace = Workspace::new();
        let buffer_id = workspace.create_text_buffer("notes");

        assert!(workspace.focus_buffer(buffer_id));
        assert!(workspace.set_buffer_contents(buffer_id, "one\ntwo"));
        assert!(workspace.append_to_buffer(buffer_id, "\nthree"));

        let content = workspace.current_buffer().text_content().unwrap();
        let lines = content.lines();
        assert_eq!(lines[0].text(), "one");
        assert_eq!(lines[1].text(), "two");
        assert_eq!(lines[2].text(), "");
        assert_eq!(lines[3].text(), "three");
    }

    #[test]
    fn non_text_buffer_rejects_text_mutation_softly() {
        let mut workspace = Workspace::new();
        let buffer_id = workspace.create_buffer(
            "rows",
            BufferContent::Records(crate::kernel::RecordsContent::default()),
        );

        assert!(!workspace.append_to_buffer(buffer_id, "not text"));
        assert!(!workspace.set_buffer_contents(buffer_id, "not text"));
    }

    #[test]
    fn browser_buffer_accepts_title_mutation_but_text_buffer_does_not() {
        let mut workspace = Workspace::new();
        let browser = workspace.create_buffer(
            "browser",
            BufferContent::Browser(BrowserContent::new(
                Some("https://example.com".to_string()),
                None,
            )),
        );
        let text = workspace.create_text_buffer("notes");

        assert!(workspace.set_browser_title(browser, "Example"));
        assert!(!workspace.set_browser_title(text, "Example"));

        assert!(matches!(
            workspace.buffer(browser).unwrap().content(),
            BufferContent::Browser(content) if content.title() == Some("Example")
        ));
    }

    #[test]
    fn browser_buffer_accepts_url_mutation_but_text_buffer_does_not() {
        let mut workspace = Workspace::new();
        let browser = workspace.create_buffer(
            "browser",
            BufferContent::Browser(BrowserContent::new(
                Some("https://before.example".to_string()),
                None,
            )),
        );
        let text = workspace.create_text_buffer("notes");

        assert!(workspace.set_browser_url(browser, "https://after.example"));
        assert!(!workspace.set_browser_url(text, "https://after.example"));

        assert!(matches!(
            workspace.buffer(browser).unwrap().content(),
            BufferContent::Browser(content) if content.url() == Some("https://after.example")
        ));
    }

    #[test]
    fn media_buffer_accepts_source_mutation_but_text_buffer_does_not() {
        let mut workspace = Workspace::new();
        let media = workspace.create_buffer(
            "media",
            BufferContent::Media(crate::kernel::MediaContent::new(Some(
                "./before.mp4".to_string(),
            ))),
        );
        let text = workspace.create_text_buffer("notes");

        assert!(workspace.set_media_source(media, "./after.mp4"));
        assert!(!workspace.set_media_source(text, "./after.mp4"));

        assert!(matches!(
            workspace.buffer(media).unwrap().content(),
            BufferContent::Media(content) if content.source() == Some("./after.mp4")
        ));
    }

    #[test]
    fn canvas_buffer_accepts_name_mutation_but_text_buffer_does_not() {
        let mut workspace = Workspace::new();
        let canvas = workspace.create_buffer(
            "canvas",
            BufferContent::Canvas(crate::kernel::CanvasContent::new(Some(
                "before".to_string(),
            ))),
        );
        let text = workspace.create_text_buffer("notes");

        assert!(workspace.set_canvas_name(canvas, "after"));
        assert!(!workspace.set_canvas_name(text, "after"));

        assert!(matches!(
            workspace.buffer(canvas).unwrap().content(),
            BufferContent::Canvas(content) if content.name() == Some("after")
        ));
    }

    #[test]
    fn splitting_active_pane_creates_view_not_content() {
        let mut workspace = Workspace::new();
        let original_buffer_id = workspace.current_buffer().id();

        let pane_id = workspace.split_active_pane(SplitAxis::Vertical);

        assert_eq!(workspace.pane_count(), 2);
        assert_eq!(workspace.buffer_count(), 1);
        assert_eq!(
            workspace.pane(pane_id).unwrap().buffer_id(),
            original_buffer_id
        );
        assert!(workspace.pane_tree().contains_pane(pane_id));
    }

    #[test]
    fn default_workspace_has_command_and_capability_scaffolding() {
        let workspace = Workspace::new();

        assert!(workspace.commands().contains("quit"));
        assert!(workspace.commands().contains("eval"));
        assert!(
            workspace
                .capabilities()
                .contains(&Capability::new(CapabilityDomain::Buffer, "*"))
        );
    }

    #[test]
    fn workspace_creates_jobs_with_stable_ids() {
        let mut workspace = Workspace::new();

        let job_id = workspace.create_job("grep", Some(JobOwner::Workspace(workspace.id())));

        let job = workspace.jobs().get(job_id).unwrap();
        assert_eq!(job.name(), "grep");
        assert_eq!(job.owner(), Some(JobOwner::Workspace(workspace.id())));
    }

    #[test]
    fn workspace_refreshes_jobs_buffer_when_jobs_change() {
        let mut workspace = Workspace::new();
        let jobs_buffer = workspace.create_records_buffer("*jobs*");
        assert!(workspace.focus_buffer(jobs_buffer));
        workspace.refresh_jobs_buffer_if_present();

        let job_id =
            workspace.create_job("index workspace", Some(JobOwner::Workspace(workspace.id())));

        assert!(matches!(
            workspace.buffer(jobs_buffer).unwrap().content(),
            BufferContent::Records(content)
                if content.records()
                    == &[serde_json::json!({
                        "id": job_id.raw(),
                        "name": "index workspace",
                        "status": "pending",
                        "owner_kind": "workspace",
                        "owner_id": workspace.id().raw(),
                        "kind": "generic",
                        "package": serde_json::Value::Null,
                        "command": serde_json::Value::Null,
                        "output_buffer_id": serde_json::Value::Null,
                        "output_buffer_name": serde_json::Value::Null,
                        "has_output": false,
                        "summary": "index workspace [pending]",
                    })]
        ));

        assert!(workspace.set_job_status(job_id, JobStatus::Running));
        assert!(matches!(
            workspace.buffer(jobs_buffer).unwrap().content(),
            BufferContent::Records(content)
                if content.records()[0]["status"] == serde_json::json!("running")
        ));

        assert!(workspace.jobs_mut().cancel(job_id));
        workspace.refresh_jobs_buffer_if_present();
        assert!(matches!(
            workspace.buffer(jobs_buffer).unwrap().content(),
            BufferContent::Records(content)
                if content.records()[0]["status"] == serde_json::json!("cancelled")
        ));
    }

    #[test]
    fn create_text_buffer_starts_empty() {
        let mut workspace = Workspace::new();

        let buffer_id = workspace.create_text_buffer("empty");

        assert_eq!(
            workspace.buffer(buffer_id).unwrap().content(),
            &BufferContent::Text(TextContent::default())
        );
    }

    #[test]
    fn focusing_adjacent_panes_cycles_through_leaf_order() {
        let mut workspace = Workspace::new();

        let second = workspace.split_active_pane(SplitAxis::Vertical);
        let third = workspace.split_active_pane(SplitAxis::Horizontal);

        assert_eq!(workspace.active_pane_id(), third);
        assert_eq!(workspace.focus_next_pane(), Some(PaneId::new(1)));
        assert_eq!(workspace.active_pane_id(), PaneId::new(1));
        assert_eq!(workspace.focus_previous_pane(), Some(third));
        assert_eq!(workspace.active_pane_id(), third);
        assert_eq!(workspace.focus_previous_pane(), Some(second));
        assert_eq!(workspace.active_pane_id(), second);
    }

    #[test]
    fn resizing_active_pane_updates_split_ratio() {
        let mut workspace = Workspace::new();

        workspace.split_active_pane(SplitAxis::Vertical);

        assert!(workspace.resize_active_pane(crate::kernel::PaneDirection::Left, 10));
        assert!(matches!(
            workspace.pane_tree().root(),
            crate::kernel::PaneNode::Split {
                ratio_percent: 40,
                ..
            }
        ));
    }

    #[test]
    fn workspace_snapshot_round_trip_preserves_rich_buffers_and_pane_state() {
        let mut workspace = Workspace::new();
        let text = workspace.create_text_buffer("notes");
        let terminal = workspace.create_buffer(
            "terminal",
            BufferContent::Terminal(TerminalContent::default()),
        );
        let browser = workspace.create_buffer(
            "browser",
            BufferContent::Browser(BrowserContent::new(
                Some("https://example.com".to_string()),
                Some("Example Domain".to_string()),
            )),
        );
        let media = workspace.create_buffer(
            "media",
            BufferContent::Media(MediaContent::new(Some("./clip.mp4".to_string()))),
        );
        let canvas = workspace.create_buffer(
            "canvas",
            BufferContent::Canvas(CanvasContent::new(Some("playground".to_string()))),
        );
        let records = workspace.create_buffer(
            "records",
            BufferContent::Records(RecordsContent::new(vec![
                serde_json::json!({"kind": "a"}),
                serde_json::json!({"kind": "b"}),
            ])),
        );
        let tree = workspace.create_buffer(
            "tree",
            BufferContent::Tree(TreeContent::new(vec![TreeNode::new("root", "Root")])),
        );
        let generic_job =
            workspace.create_job("index workspace", Some(JobOwner::Workspace(workspace.id())));
        assert!(workspace.set_job_status(generic_job, JobStatus::Running));
        let package_job = workspace.create_job_with_kind(
            "package echo default",
            Some(JobOwner::Workspace(workspace.id())),
            JobKind::PackageInvoke {
                package: "echo".to_string(),
                command: "default".to_string(),
                output_buffer_id: records,
            },
        );
        assert!(workspace.set_job_status(package_job, JobStatus::Succeeded));

        assert!(workspace.set_buffer_contents(text, "one\ntwo"));
        assert!(workspace.append_to_terminal_buffer(terminal, "hello\nworld"));
        workspace.set_status("Snapshot ready");
        workspace.messages_mut().info("saved info");
        workspace.messages_mut().warn("saved warning");

        assert!(workspace.focus_buffer(browser));
        let browser_pane = workspace.active_pane_id();
        workspace
            .pane_mut(browser_pane)
            .expect("browser pane should exist")
            .set_viewport(Viewport::new(3, 7));
        workspace
            .pane_mut(browser_pane)
            .expect("browser pane should exist")
            .set_presentation(PanePresentation::Preview);
        workspace
            .pane_mut(browser_pane)
            .expect("browser pane should exist")
            .set_selection(Some(Selection::Surface(SurfaceSelection::new("preview"))));

        let terminal_pane = workspace.split_active_pane(SplitAxis::Vertical);
        assert_eq!(workspace.active_pane_id(), terminal_pane);
        assert!(workspace.focus_buffer(terminal));

        let records_pane = workspace.split_active_pane(SplitAxis::Horizontal);
        assert_eq!(workspace.active_pane_id(), records_pane);
        assert!(workspace.focus_buffer(records));

        let snapshot = workspace.snapshot();
        let json = serde_json::to_string(&snapshot).expect("snapshot should serialize");
        let restored_snapshot = serde_json::from_str(&json).expect("snapshot should deserialize");
        let restored =
            Workspace::from_snapshot(restored_snapshot).expect("snapshot should restore");

        assert_eq!(restored.id(), workspace.id());
        assert_eq!(restored.buffer_count(), workspace.buffer_count());
        assert_eq!(restored.pane_count(), workspace.pane_count());
        assert_eq!(restored.active_pane_id(), records_pane);
        assert_eq!(restored.current_buffer().id(), records);
        assert!(restored.commands().contains("terminal.append"));
        assert_eq!(restored.minibuffer().input(), "Snapshot ready");
        assert_eq!(restored.messages().entries().len(), 2);
        assert_eq!(restored.messages().entries()[0].text(), "saved info");
        assert_eq!(restored.messages().entries()[1].text(), "saved warning");
        assert_eq!(restored.jobs().entries().count(), 2);
        assert_eq!(
            restored.jobs().get(generic_job).unwrap().name(),
            "index workspace"
        );
        assert_eq!(
            restored.jobs().get(generic_job).unwrap().status(),
            &JobStatus::Running
        );
        assert_eq!(
            restored.jobs().get(package_job).unwrap().status(),
            &JobStatus::Succeeded
        );
        assert_eq!(
            restored.jobs().get(package_job).unwrap().kind(),
            &JobKind::PackageInvoke {
                package: "echo".to_string(),
                command: "default".to_string(),
                output_buffer_id: records,
            }
        );

        assert!(matches!(
            restored.buffer(text).unwrap().content(),
            BufferContent::Text(content)
                if content.lines().iter().map(|line| line.text()).collect::<Vec<_>>() == vec!["one", "two"]
        ));
        assert!(matches!(
            restored.buffer(terminal).unwrap().content(),
            BufferContent::Terminal(content)
                if content.transcript().lines().iter().map(|line| line.text()).collect::<Vec<_>>()
                    == vec!["hello", "world"]
        ));
        assert!(matches!(
            restored.buffer(browser).unwrap().content(),
            BufferContent::Browser(content)
                if content.url() == Some("https://example.com")
                    && content.title() == Some("Example Domain")
        ));
        assert!(matches!(
            restored.buffer(media).unwrap().content(),
            BufferContent::Media(content) if content.source() == Some("./clip.mp4")
        ));
        assert!(matches!(
            restored.buffer(canvas).unwrap().content(),
            BufferContent::Canvas(content) if content.name() == Some("playground")
        ));
        assert!(matches!(
            restored.buffer(records).unwrap().content(),
            BufferContent::Records(content) if content.records().len() == 2
        ));
        assert!(matches!(
            restored.buffer(tree).unwrap().content(),
            BufferContent::Tree(content)
                if content.roots().iter().map(|node| node.label()).collect::<Vec<_>>() == vec!["Root"]
        ));

        let restored_browser_pane = restored
            .pane(browser_pane)
            .expect("browser pane should restore");
        assert_eq!(restored_browser_pane.buffer_id(), browser);
        assert_eq!(restored_browser_pane.viewport(), Viewport::new(3, 7));
        assert_eq!(
            restored_browser_pane.presentation(),
            PanePresentation::Preview
        );
        assert_eq!(
            restored_browser_pane.selection(),
            Some(&Selection::Surface(SurfaceSelection::new("preview")))
        );
        assert_eq!(restored.pane_tree(), workspace.pane_tree());

        let next_job = {
            let mut restored = restored;
            restored.create_job("follow-up", Some(JobOwner::Workspace(restored.id())))
        };
        assert_eq!(next_job.raw(), package_job.raw() + 1);
    }

    #[test]
    fn workspace_snapshot_file_round_trip_preserves_state() {
        let mut workspace = Workspace::new();
        let terminal = workspace.create_buffer(
            "terminal",
            BufferContent::Terminal(TerminalContent::default()),
        );
        let job_id = workspace.create_job_with_kind(
            "package echo default",
            Some(JobOwner::Workspace(workspace.id())),
            JobKind::PackageInvoke {
                package: "echo".to_string(),
                command: "default".to_string(),
                output_buffer_id: terminal,
            },
        );
        assert!(workspace.set_job_status(job_id, JobStatus::Failed("package failed".to_string())));
        assert!(workspace.focus_buffer(terminal));
        assert!(workspace.append_to_terminal_buffer(terminal, "hello"));
        workspace.set_status("Saved to disk");
        workspace.messages_mut().info("persisted");

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("zred-workspace-{unique}.json"));

        workspace
            .save_snapshot_file(&path)
            .expect("snapshot file should save");
        let restored = Workspace::load_snapshot_file(&path).expect("snapshot file should load");
        std::fs::remove_file(&path).expect("snapshot file should be removed");

        assert_eq!(restored.current_buffer().id(), terminal);
        assert_eq!(restored.minibuffer().input(), "Saved to disk");
        assert_eq!(restored.messages().entries()[0].text(), "persisted");
        assert_eq!(
            restored.jobs().get(job_id).unwrap().status(),
            &JobStatus::Failed("package failed".to_string())
        );
        assert!(matches!(
            restored.current_buffer().content(),
            BufferContent::Terminal(content)
                if content.transcript().lines().iter().map(|line| line.text()).collect::<Vec<_>>()
                    == vec!["hello"]
        ));
    }

    #[test]
    fn workspace_restore_rejects_unsupported_snapshot_version() {
        let snapshot = WorkspaceSnapshot {
            version: crate::kernel::workspace::snapshot::WORKSPACE_SNAPSHOT_VERSION + 1,
            workspace_id: WorkspaceId::new(1),
            buffers: vec![BufferSnapshot {
                id: BufferId::new(1),
                name: "notes".to_string(),
                content: BufferContent::Text(TextContent::from_text("notes")),
            }],
            jobs: Vec::new(),
            panes: vec![PaneSnapshot {
                id: PaneId::new(1),
                buffer_id: BufferId::new(1),
                viewport: Viewport::default(),
                presentation: PanePresentation::Default,
                selection: None,
            }],
            pane_tree: crate::kernel::PaneTree::single(PaneId::new(1)),
            active_pane: PaneId::new(1),
            minibuffer: crate::kernel::Minibuffer::message("Ready"),
            messages: crate::kernel::MessageLog::new(),
        };

        let result = Workspace::from_snapshot(snapshot);

        assert!(matches!(
            result,
            Err(WorkspaceRestoreError::UnsupportedVersion(version))
                if version == crate::kernel::workspace::snapshot::WORKSPACE_SNAPSHOT_VERSION + 1
        ));
    }

    #[test]
    fn workspace_restore_rejects_missing_pane_buffer_references() {
        let snapshot = WorkspaceSnapshot {
            version: crate::kernel::workspace::snapshot::WORKSPACE_SNAPSHOT_VERSION,
            workspace_id: WorkspaceId::new(1),
            buffers: vec![BufferSnapshot {
                id: BufferId::new(1),
                name: "notes".to_string(),
                content: BufferContent::Text(TextContent::from_text("notes")),
            }],
            jobs: Vec::new(),
            panes: vec![PaneSnapshot {
                id: PaneId::new(1),
                buffer_id: BufferId::new(2),
                viewport: Viewport::default(),
                presentation: PanePresentation::Default,
                selection: None,
            }],
            pane_tree: crate::kernel::PaneTree::single(PaneId::new(1)),
            active_pane: PaneId::new(1),
            minibuffer: crate::kernel::Minibuffer::message("Ready"),
            messages: crate::kernel::MessageLog::new(),
        };

        let result = Workspace::from_snapshot(snapshot);

        assert!(matches!(
            result,
            Err(WorkspaceRestoreError::MissingPaneBuffer {
                pane_id,
                buffer_id,
            }) if pane_id == PaneId::new(1) && buffer_id == BufferId::new(2)
        ));
    }

    #[test]
    fn default_workspace_keymaps_expose_message_mode_bindings() {
        let workspace = Workspace::new();

        assert_eq!(
            workspace.lookup_keymap(&[KeyChord::new(
                KeyCodeRepr::Char(':'),
                KeyModifiersRepr::NONE,
            )]),
            KeymapLookup::Matched("minibuffer.command.enter")
        );
        assert_eq!(
            workspace.lookup_keymap(&[KeyChord::new(
                KeyCodeRepr::Char('n'),
                KeyModifiersRepr::NONE,
            )]),
            KeymapLookup::Matched("buffer.new.next")
        );
    }

    #[test]
    fn default_workspace_keymaps_expose_ctrl_w_sequences() {
        let workspace = Workspace::new();

        assert_eq!(
            workspace.lookup_keymap(&[KeyChord::new(
                KeyCodeRepr::Char('w'),
                KeyModifiersRepr::CONTROL,
            )]),
            KeymapLookup::Pending
        );
        assert_eq!(
            workspace.lookup_keymap(&[
                KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
                KeyChord::new(KeyCodeRepr::Char('v'), KeyModifiersRepr::NONE),
            ]),
            KeymapLookup::Matched("pane.split.vertical")
        );
        assert_eq!(
            workspace.lookup_keymap(&[
                KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
                KeyChord::new(KeyCodeRepr::Char('v'), KeyModifiersRepr::CONTROL),
            ]),
            KeymapLookup::Matched("pane.split.vertical")
        );
    }
}
