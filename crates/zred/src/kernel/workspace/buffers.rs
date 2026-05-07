use super::Workspace;
use crate::kernel::buffer::{Buffer, BufferContent, TreeNode};
use crate::kernel::ids::{BufferId, PaneId};
use crate::kernel::pane::Pane;
use crate::kernel::pane_tree::{PaneDirection, SplitAxis};
use crate::kernel::selection::{RecordSelection, Selection};
use serde_json::json;

const JOBS_BUFFER_NAME: &str = "*jobs*";

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

    pub fn find_buffer_by_name(&self, name: &str) -> Option<BufferId> {
        self.buffers
            .iter()
            .find_map(|(id, buffer)| (buffer.name() == name).then_some(*id))
    }

    pub fn job_records(&self) -> Vec<serde_json::Value> {
        self.jobs.entries().map(job_record).collect()
    }

    pub fn refresh_jobs_buffer_if_present(&mut self) {
        let Some(buffer_id) = self.find_buffer_by_name(JOBS_BUFFER_NAME) else {
            return;
        };

        let records = self.job_records();
        let _ = self.set_records_buffer_contents(buffer_id, records);
    }

    pub fn select_record_row(&mut self, row: usize) -> bool {
        let is_valid_row = matches!(
            self.current_buffer().content(),
            BufferContent::Records(content) if row < content.records().len()
        );
        if !is_valid_row {
            return false;
        }

        let Some(pane) = self.panes.get_mut(&self.active_pane) else {
            return false;
        };

        pane.set_selection(Some(Selection::Records(RecordSelection::new(vec![row]))));
        true
    }

    pub fn selected_record_row(&self) -> Option<usize> {
        let Selection::Records(selection) = self.active_pane().selection()? else {
            return None;
        };
        selection.rows().first().copied()
    }

    pub fn current_record(&self) -> Option<&serde_json::Value> {
        let BufferContent::Records(content) = self.current_buffer().content() else {
            return None;
        };
        let row = self.selected_record_row().unwrap_or(0);
        content.records().get(row)
    }

    pub fn linked_buffer_id_from_current_record(&self) -> Option<BufferId> {
        let record = self.current_record()?;
        record
            .get("buffer_id")
            .or_else(|| record.get("output_buffer_id"))?
            .as_u64()
            .map(BufferId::new)
    }

    pub fn selected_tree_node_id(&self) -> Option<String> {
        let crate::kernel::Selection::Tree(selection) = self.active_pane().selection()? else {
            return None;
        };
        selection.node_ids().first().cloned()
    }

    pub fn current_tree_node(&self) -> Option<&TreeNode> {
        let selected_id = self.selected_tree_node_id()?;
        let BufferContent::Tree(content) = self.current_buffer().content() else {
            return None;
        };
        find_tree_node(content.roots(), &selected_id)
    }

    pub fn linked_buffer_id_from_current_tree_node(&self) -> Option<BufferId> {
        self.current_tree_node()?.linked_buffer_id()
    }

    pub fn select_tree_node(&mut self, node_id: impl Into<String>) -> bool {
        let node_id = node_id.into();
        let BufferContent::Tree(content) = self.current_buffer().content() else {
            return false;
        };
        if find_tree_node(content.roots(), &node_id).is_none() {
            return false;
        }

        let Some(pane) = self.panes.get_mut(&self.active_pane) else {
            return false;
        };
        pane.set_selection(Some(Selection::Tree(crate::kernel::TreeSelection::new(
            vec![node_id],
        ))));
        true
    }

    pub fn move_selected_tree_node(&mut self, delta: isize) -> Option<String> {
        let BufferContent::Tree(content) = self.current_buffer().content() else {
            return None;
        };
        let node_ids = flatten_tree_node_ids(content.roots());
        if node_ids.is_empty() {
            return None;
        }

        let current_index = self
            .selected_tree_node_id()
            .and_then(|selected| node_ids.iter().position(|id| id == &selected))
            .unwrap_or(0);
        let len = node_ids.len() as isize;
        let next_index = (current_index as isize + delta).rem_euclid(len) as usize;
        let next_id = node_ids[next_index].clone();
        let pane = self.panes.get_mut(&self.active_pane)?;
        pane.set_selection(Some(Selection::Tree(crate::kernel::TreeSelection::new(
            vec![next_id.clone()],
        ))));
        Some(next_id)
    }

    pub fn selected_job_id_from_jobs_buffer(&self) -> Option<crate::kernel::JobId> {
        if self.current_buffer().name() != JOBS_BUFFER_NAME {
            return None;
        }

        let Selection::Records(selection) = self.active_pane().selection()? else {
            return None;
        };
        let row = *selection.rows().first()?;
        let BufferContent::Records(content) = self.current_buffer().content() else {
            return None;
        };

        content
            .records()
            .get(row)?
            .get("id")?
            .as_u64()
            .map(crate::kernel::JobId::new)
    }

    pub fn move_selected_job_row(&mut self, delta: isize) -> Option<usize> {
        if self.current_buffer().name() != JOBS_BUFFER_NAME {
            return None;
        }

        self.move_selected_record_row(delta)
    }

    pub fn move_selected_record_row(&mut self, delta: isize) -> Option<usize> {
        let BufferContent::Records(content) = self.current_buffer().content() else {
            return None;
        };
        if content.records().is_empty() {
            return None;
        }

        let current_row = self.selected_record_row().unwrap_or(0);
        let len = content.records().len() as isize;
        let next_row = (current_row as isize + delta).rem_euclid(len) as usize;

        let pane = self.panes.get_mut(&self.active_pane)?;
        pane.set_selection(Some(Selection::Records(RecordSelection::new(vec![
            next_row,
        ]))));
        Some(next_row)
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

    pub fn set_browser_title(&mut self, buffer_id: BufferId, title: &str) -> bool {
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return false;
        };

        buffer.set_browser_title(title)
    }

    pub fn set_browser_url(&mut self, buffer_id: BufferId, url: &str) -> bool {
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return false;
        };

        buffer.set_browser_url(url)
    }

    pub fn set_media_source(&mut self, buffer_id: BufferId, source: &str) -> bool {
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return false;
        };

        buffer.set_media_source(source)
    }

    pub fn set_canvas_name(&mut self, buffer_id: BufferId, name: &str) -> bool {
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return false;
        };

        buffer.set_canvas_name(name)
    }

    pub fn append_to_terminal_buffer(&mut self, buffer_id: BufferId, text: &str) -> bool {
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return false;
        };

        buffer.append_terminal_text(text)
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

    pub fn set_records_buffer_contents(
        &mut self,
        buffer_id: BufferId,
        records: Vec<serde_json::Value>,
    ) -> bool {
        let Some(buffer) = self.buffers.get_mut(&buffer_id) else {
            return false;
        };

        buffer.set_records(records)
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

    pub fn focus_pane(&mut self, pane_id: PaneId) -> bool {
        if !self.panes.contains_key(&pane_id) || !self.pane_tree.contains_pane(pane_id) {
            return false;
        }

        self.active_pane = pane_id;
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

fn job_record(job: &crate::kernel::Job) -> serde_json::Value {
    let (owner_kind, owner_id) = match job.owner() {
        Some(crate::kernel::JobOwner::Workspace(id)) => (Some("workspace"), Some(id.raw())),
        Some(crate::kernel::JobOwner::Buffer(id)) => (Some("buffer"), Some(id.raw())),
        Some(crate::kernel::JobOwner::Pane(id)) => (Some("pane"), Some(id.raw())),
        None => (None, None),
    };
    let (kind, package, command, output_buffer_id) = match job.kind() {
        crate::kernel::JobKind::Generic => ("generic", None, None, None),
        crate::kernel::JobKind::PackageInvoke {
            package,
            command,
            output_buffer_id,
        } => (
            "package_invoke",
            Some(package.clone()),
            Some(command.clone()),
            Some(output_buffer_id.raw()),
        ),
    };
    let has_output = output_buffer_id.is_some();
    let output_buffer_name = output_buffer_id.and_then(|_| match job.kind() {
        crate::kernel::JobKind::PackageInvoke {
            package, command, ..
        } => Some(format!("*pkg:{package} {command}*")),
        crate::kernel::JobKind::Generic => None,
    });
    let summary = match (&package, &command, output_buffer_id) {
        (Some(package), Some(command), Some(output_buffer_id)) => format!(
            "{} [{}] {} {} -> buffer {}",
            job.name(),
            match job.status() {
                crate::kernel::JobStatus::Pending => "pending".to_string(),
                crate::kernel::JobStatus::Running => "running".to_string(),
                crate::kernel::JobStatus::Succeeded => "succeeded".to_string(),
                crate::kernel::JobStatus::Failed(message) => format!("failed: {message}"),
                crate::kernel::JobStatus::Cancelled => "cancelled".to_string(),
            },
            package,
            command,
            output_buffer_id
        ),
        _ => format!(
            "{} [{}]",
            job.name(),
            match job.status() {
                crate::kernel::JobStatus::Pending => "pending".to_string(),
                crate::kernel::JobStatus::Running => "running".to_string(),
                crate::kernel::JobStatus::Succeeded => "succeeded".to_string(),
                crate::kernel::JobStatus::Failed(message) => format!("failed: {message}"),
                crate::kernel::JobStatus::Cancelled => "cancelled".to_string(),
            }
        ),
    };

    json!({
        "id": job.id().raw(),
        "name": job.name(),
        "status": match job.status() {
            crate::kernel::JobStatus::Pending => "pending".to_string(),
            crate::kernel::JobStatus::Running => "running".to_string(),
            crate::kernel::JobStatus::Succeeded => "succeeded".to_string(),
            crate::kernel::JobStatus::Failed(message) => format!("failed: {message}"),
            crate::kernel::JobStatus::Cancelled => "cancelled".to_string(),
        },
        "owner_kind": owner_kind,
        "owner_id": owner_id,
        "kind": kind,
        "package": package,
        "command": command,
        "output_buffer_id": output_buffer_id,
        "output_buffer_name": output_buffer_name,
        "has_output": has_output,
        "summary": summary,
    })
}

fn flatten_tree_node_ids(roots: &[TreeNode]) -> Vec<String> {
    let mut ids = Vec::new();
    for root in roots {
        collect_tree_node_ids(root, &mut ids);
    }
    ids
}

fn collect_tree_node_ids(node: &TreeNode, ids: &mut Vec<String>) {
    ids.push(node.id().to_string());
    for child in node.children() {
        collect_tree_node_ids(child, ids);
    }
}

fn find_tree_node<'a>(nodes: &'a [TreeNode], id: &str) -> Option<&'a TreeNode> {
    for node in nodes {
        if node.id() == id {
            return Some(node);
        }
        if let Some(found) = find_tree_node(node.children(), id) {
            return Some(found);
        }
    }
    None
}
