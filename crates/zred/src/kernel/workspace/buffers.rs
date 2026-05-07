use super::Workspace;
use crate::kernel::buffer::{Buffer, BufferContent};
use crate::kernel::ids::{BufferId, PaneId};
use crate::kernel::pane::Pane;
use crate::kernel::pane_tree::{PaneDirection, SplitAxis};

impl Workspace {
    pub fn create_buffer(&mut self, name: &str, content: BufferContent) -> BufferId {
        let id = self.ids.next_buffer_id();
        self.buffers.insert(id, Buffer::new(id, name, content));
        id
    }

    pub fn create_text_buffer(&mut self, name: &str) -> BufferId {
        self.create_buffer(name, BufferContent::Text(Default::default()))
    }

    pub fn create_records_buffer(&mut self, name: &str) -> BufferId {
        self.create_buffer(name, BufferContent::Records(Default::default()))
    }

    pub fn append_to_buffer(&mut self, buffer_id: BufferId, text: &str) -> bool {
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return false;
        };

        buffer.append_text(text)
    }

    pub fn set_buffer_contents(&mut self, buffer_id: BufferId, text: &str) -> bool {
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return false;
        };

        buffer.set_text(text)
    }

    pub fn push_record_to_buffer(
        &mut self,
        buffer_id: BufferId,
        record: serde_json::Value,
    ) -> bool {
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return false;
        };

        buffer.push_record(record)
    }

    pub fn focus_buffer(&mut self, buffer_id: BufferId) -> bool {
        if !self.buffers.contains_key(&buffer_id) {
            return false;
        }

        let Some(pane) = self.panes.get_mut(&self.active_pane) else {
            return false;
        };

        pane.set_buffer_id(buffer_id);
        true
    }

    pub fn split_active_pane(&mut self, axis: SplitAxis) -> PaneId {
        let new_pane_id = self.ids.next_pane_id();
        let buffer_id = self.active_pane().buffer_id();

        self.panes
            .insert(new_pane_id, Pane::new(new_pane_id, buffer_id));
        let split = self
            .pane_tree
            .split_leaf(self.active_pane, new_pane_id, axis);
        debug_assert!(split, "active pane should be present in pane tree");
        self.active_pane = new_pane_id;
        new_pane_id
    }

    pub fn focus_next_pane(&mut self) -> Option<PaneId> {
        self.focus_adjacent_pane(1)
    }

    pub fn focus_previous_pane(&mut self) -> Option<PaneId> {
        self.focus_adjacent_pane(-1)
    }

    pub fn focus_pane_direction(&mut self, direction: PaneDirection) -> Option<PaneId> {
        let pane_id = self.pane_tree.adjacent_pane(self.active_pane, direction)?;
        self.active_pane = pane_id;
        Some(pane_id)
    }

    pub fn resize_active_pane(&mut self, direction: PaneDirection, delta_percent: u8) -> bool {
        self.pane_tree
            .resize_pane(self.active_pane, direction, delta_percent)
    }

    fn focus_adjacent_pane(&mut self, step: isize) -> Option<PaneId> {
        let leaves = self.pane_tree.leaves();
        if leaves.len() <= 1 {
            return None;
        }

        let current_index = leaves
            .iter()
            .position(|pane_id| *pane_id == self.active_pane)?;
        let len = leaves.len() as isize;
        let next_index = (current_index as isize + step).rem_euclid(len) as usize;
        let next_pane = leaves[next_index];
        self.active_pane = next_pane;
        Some(next_pane)
    }
}
