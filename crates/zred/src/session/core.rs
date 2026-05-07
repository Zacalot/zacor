use crate::kernel::{Minibuffer, Workspace};
use std::cell::{Ref, RefCell};
use std::rc::Rc;

pub type SessionResult<T> = Result<T, String>;

pub type SharedSession = Rc<RefCell<Session>>;

pub struct Session {
    pub(super) workspace: Workspace,
    pub(super) should_quit: bool,
}

impl Session {
    pub fn new() -> Self {
        Self {
            workspace: Workspace::new(),
            should_quit: false,
        }
    }

    pub fn shared() -> SharedSession {
        Rc::new(RefCell::new(Self::new()))
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

    #[cfg(test)]
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspace
    }
}
