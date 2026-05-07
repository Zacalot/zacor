use crate::kernel::{Minibuffer, Workspace, WorkspaceId};
use std::cell::{Ref, RefCell};
use std::rc::Rc;

pub type SessionResult<T> = Result<T, String>;

pub type SharedSession = Rc<RefCell<Session>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionFrontendEffect {
    NewWindow,
}

pub struct Session {
    pub(super) workspace: Workspace,
    pub(super) should_quit: bool,
    pub(super) pending_frontend_effects: Vec<SessionFrontendEffect>,
}

impl Session {
    pub fn new() -> Self {
        Self::with_workspace(Workspace::new())
    }

    pub fn with_workspace_id(workspace_id: WorkspaceId) -> Self {
        Self::with_workspace(Workspace::with_id(workspace_id))
    }

    fn with_workspace(workspace: Workspace) -> Self {
        Self {
            workspace,
            should_quit: false,
            pending_frontend_effects: Vec::new(),
        }
    }

    pub fn shared() -> SharedSession {
        Rc::new(RefCell::new(Self::new()))
    }

    pub fn shared_with_workspace_id(workspace_id: WorkspaceId) -> SharedSession {
        Rc::new(RefCell::new(Self::with_workspace_id(workspace_id)))
    }

    pub fn borrow(shared: &SharedSession) -> Ref<'_, Session> {
        shared.borrow()
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.workspace.set_status(status);
    }

    pub fn request_quit(&mut self) {
        self.should_quit = true;
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    #[allow(dead_code)]
    pub fn current_buffer(&self) -> &crate::kernel::Buffer {
        self.workspace.current_buffer()
    }

    pub fn buffer_count(&self) -> usize {
        self.workspace.buffer_count()
    }

    #[allow(dead_code)]
    pub fn pane_count(&self) -> usize {
        self.workspace.pane_count()
    }

    #[allow(dead_code)]
    pub fn active_pane_id(&self) -> u64 {
        self.workspace.active_pane_id().raw()
    }

    pub fn minibuffer(&self) -> &Minibuffer {
        self.workspace.minibuffer()
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspace
    }

    pub fn replace_workspace(&mut self, workspace: Workspace) {
        self.workspace = workspace;
    }

    pub fn push_frontend_effect(&mut self, effect: SessionFrontendEffect) {
        self.pending_frontend_effects.push(effect);
    }

    pub fn drain_frontend_effects(&mut self) -> Vec<SessionFrontendEffect> {
        self.pending_frontend_effects.drain(..).collect()
    }
}
