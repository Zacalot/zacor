use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! define_id {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
        )]
        pub struct $name(u64);

        impl $name {
            pub const fn new(raw: u64) -> Self {
                Self(raw)
            }

            #[allow(dead_code)]
            pub const fn raw(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

define_id!(BufferId);
define_id!(PaneId);
define_id!(WorkspaceId);
define_id!(JobId);

#[derive(Clone, Debug)]
pub struct IdAllocator {
    next_buffer_id: u64,
    next_pane_id: u64,
    next_workspace_id: u64,
    next_job_id: u64,
}

impl IdAllocator {
    pub fn new() -> Self {
        Self {
            next_buffer_id: 1,
            next_pane_id: 1,
            next_workspace_id: 1,
            next_job_id: 1,
        }
    }

    pub fn next_buffer_id(&mut self) -> BufferId {
        let id = BufferId::new(self.next_buffer_id);
        self.next_buffer_id += 1;
        id
    }

    pub fn next_pane_id(&mut self) -> PaneId {
        let id = PaneId::new(self.next_pane_id);
        self.next_pane_id += 1;
        id
    }

    pub fn next_workspace_id(&mut self) -> WorkspaceId {
        let id = WorkspaceId::new(self.next_workspace_id);
        self.next_workspace_id += 1;
        id
    }

    pub fn next_job_id(&mut self) -> JobId {
        let id = JobId::new(self.next_job_id);
        self.next_job_id += 1;
        id
    }
}

impl Default for IdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl IdAllocator {
    pub fn from_next_ids(
        next_buffer_id: u64,
        next_pane_id: u64,
        next_workspace_id: u64,
        next_job_id: u64,
    ) -> Self {
        Self {
            next_buffer_id,
            next_pane_id,
            next_workspace_id,
            next_job_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_stable_typed_ids_independently() {
        let mut ids = IdAllocator::new();

        assert_eq!(ids.next_buffer_id(), BufferId::new(1));
        assert_eq!(ids.next_buffer_id(), BufferId::new(2));
        assert_eq!(ids.next_pane_id(), PaneId::new(1));
        assert_eq!(ids.next_workspace_id(), WorkspaceId::new(1));
        assert_eq!(ids.next_job_id(), JobId::new(1));
    }
}
