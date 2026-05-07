mod buffers;
mod commands;

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
        let mut ids = IdAllocator::new();
        let id = ids.next_workspace_id();
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
        "buffer.new",
        "Create a text buffer",
        CommandScope::Workspace,
        CommandSpec::buffer_new(),
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
    keymaps.bind(
        vec![
            KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
            KeyChord::new(KeyCodeRepr::Char('h'), KeyModifiersRepr::NONE),
        ],
        "pane.left",
    );
    keymaps.bind(
        vec![
            KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
            KeyChord::new(KeyCodeRepr::Char('j'), KeyModifiersRepr::NONE),
        ],
        "pane.down",
    );
    keymaps.bind(
        vec![
            KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
            KeyChord::new(KeyCodeRepr::Char('k'), KeyModifiersRepr::NONE),
        ],
        "pane.up",
    );
    keymaps.bind(
        vec![
            KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
            KeyChord::new(KeyCodeRepr::Char('l'), KeyModifiersRepr::NONE),
        ],
        "pane.right",
    );
    keymaps.bind(
        vec![
            KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
            KeyChord::new(KeyCodeRepr::Char('H'), KeyModifiersRepr::NONE),
        ],
        "pane.resize.left",
    );
    keymaps.bind(
        vec![
            KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
            KeyChord::new(KeyCodeRepr::Char('J'), KeyModifiersRepr::NONE),
        ],
        "pane.resize.down",
    );
    keymaps.bind(
        vec![
            KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
            KeyChord::new(KeyCodeRepr::Char('K'), KeyModifiersRepr::NONE),
        ],
        "pane.resize.up",
    );
    keymaps.bind(
        vec![
            KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
            KeyChord::new(KeyCodeRepr::Char('L'), KeyModifiersRepr::NONE),
        ],
        "pane.resize.right",
    );
    keymaps.bind(
        vec![
            KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
            KeyChord::new(KeyCodeRepr::Char('n'), KeyModifiersRepr::NONE),
        ],
        "pane.next",
    );
    keymaps.bind(
        vec![
            KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
            KeyChord::new(KeyCodeRepr::Char('p'), KeyModifiersRepr::NONE),
        ],
        "pane.prev",
    );
    keymaps.bind(
        vec![
            KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
            KeyChord::new(KeyCodeRepr::Char('s'), KeyModifiersRepr::NONE),
        ],
        "pane.split.horizontal",
    );
    keymaps.bind(
        vec![
            KeyChord::new(KeyCodeRepr::Char('w'), KeyModifiersRepr::CONTROL),
            KeyChord::new(KeyCodeRepr::Char('v'), KeyModifiersRepr::NONE),
        ],
        "pane.split.vertical",
    );
    keymaps
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
    use crate::kernel::SplitAxis;
    use crate::kernel::buffer::{
        BufferContent, BufferKind, SCRATCH_BUFFER_NAME, SCRATCH_BUFFER_TEXT, TextContent,
    };
    use crate::kernel::{KeyChord, KeyCodeRepr, KeyModifiersRepr, KeymapLookup};

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
    }
}
