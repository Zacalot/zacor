use super::Workspace;
use crate::kernel::{
    Buffer, BufferContent, BufferId, IdAllocator, Job, JobId, JobKind, JobOwner, JobRegistry,
    JobStatus, MessageLog, Minibuffer, Pane, PaneId, PanePresentation, PaneTree, Selection,
    Viewport, WorkspaceId,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::Path;

pub const WORKSPACE_SNAPSHOT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub version: u32,
    pub workspace_id: WorkspaceId,
    pub buffers: Vec<BufferSnapshot>,
    pub jobs: Vec<JobSnapshot>,
    pub panes: Vec<PaneSnapshot>,
    pub pane_tree: PaneTree,
    pub active_pane: PaneId,
    pub minibuffer: Minibuffer,
    pub messages: MessageLog,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BufferSnapshot {
    pub id: BufferId,
    pub name: String,
    pub content: BufferContent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaneSnapshot {
    pub id: PaneId,
    pub buffer_id: BufferId,
    pub viewport: Viewport,
    pub presentation: PanePresentation,
    pub selection: Option<Selection>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JobSnapshot {
    pub id: JobId,
    pub name: String,
    pub status: JobStatus,
    pub owner: Option<JobOwner>,
    pub kind: JobKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspaceRestoreError {
    NoBuffers,
    NoPanes,
    MissingActivePane(PaneId),
    ActivePaneMissingFromTree(PaneId),
    PaneTreeMismatch,
    UnsupportedVersion(u32),
    Io(String),
    Json(String),
    MissingPaneBuffer {
        pane_id: PaneId,
        buffer_id: BufferId,
    },
}

impl fmt::Display for WorkspaceRestoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBuffers => write!(f, "workspace snapshot must contain at least one buffer"),
            Self::NoPanes => write!(f, "workspace snapshot must contain at least one pane"),
            Self::MissingActivePane(pane_id) => {
                write!(f, "workspace snapshot is missing active pane: {pane_id}")
            }
            Self::ActivePaneMissingFromTree(pane_id) => {
                write!(
                    f,
                    "workspace snapshot active pane is not in pane tree: {pane_id}"
                )
            }
            Self::PaneTreeMismatch => {
                write!(f, "workspace snapshot panes do not match pane tree leaves")
            }
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported workspace snapshot version: {version}")
            }
            Self::Io(error) => write!(f, "workspace snapshot io error: {error}"),
            Self::Json(error) => write!(f, "workspace snapshot json error: {error}"),
            Self::MissingPaneBuffer { pane_id, buffer_id } => write!(
                f,
                "workspace snapshot pane {pane_id} references missing buffer {buffer_id}"
            ),
        }
    }
}

impl Error for WorkspaceRestoreError {}

impl Workspace {
    pub fn snapshot(&self) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            version: WORKSPACE_SNAPSHOT_VERSION,
            workspace_id: self.id,
            buffers: self
                .buffers
                .values()
                .map(BufferSnapshot::from_buffer)
                .collect(),
            jobs: self.jobs.entries().map(JobSnapshot::from_job).collect(),
            panes: self.panes.values().map(PaneSnapshot::from_pane).collect(),
            pane_tree: self.pane_tree.clone(),
            active_pane: self.active_pane,
            minibuffer: self.minibuffer.clone(),
            messages: self.messages.clone(),
        }
    }

    pub fn save_snapshot_file(&self, path: impl AsRef<Path>) -> Result<(), WorkspaceRestoreError> {
        let snapshot = self.snapshot();
        snapshot.save_to_path(path)
    }

    pub fn load_snapshot_file(path: impl AsRef<Path>) -> Result<Self, WorkspaceRestoreError> {
        WorkspaceSnapshot::load_from_path(path).and_then(Self::from_snapshot)
    }

    pub fn from_snapshot(snapshot: WorkspaceSnapshot) -> Result<Self, WorkspaceRestoreError> {
        if snapshot.version != WORKSPACE_SNAPSHOT_VERSION {
            return Err(WorkspaceRestoreError::UnsupportedVersion(snapshot.version));
        }
        if snapshot.buffers.is_empty() {
            return Err(WorkspaceRestoreError::NoBuffers);
        }
        if snapshot.panes.is_empty() {
            return Err(WorkspaceRestoreError::NoPanes);
        }

        let buffers = snapshot
            .buffers
            .into_iter()
            .map(BufferSnapshot::into_buffer)
            .map(|buffer| (buffer.id(), buffer))
            .collect::<BTreeMap<_, _>>();
        let panes = snapshot
            .panes
            .into_iter()
            .map(PaneSnapshot::into_pane)
            .map(|pane| (pane.id(), pane))
            .collect::<BTreeMap<_, _>>();
        let jobs = snapshot.jobs.into_iter().map(JobSnapshot::into_job).fold(
            JobRegistry::new(),
            |mut registry, job| {
                registry.insert(job);
                registry
            },
        );

        if !panes.contains_key(&snapshot.active_pane) {
            return Err(WorkspaceRestoreError::MissingActivePane(
                snapshot.active_pane,
            ));
        }
        if !snapshot.pane_tree.contains_pane(snapshot.active_pane) {
            return Err(WorkspaceRestoreError::ActivePaneMissingFromTree(
                snapshot.active_pane,
            ));
        }

        for pane in panes.values() {
            if !buffers.contains_key(&pane.buffer_id()) {
                return Err(WorkspaceRestoreError::MissingPaneBuffer {
                    pane_id: pane.id(),
                    buffer_id: pane.buffer_id(),
                });
            }
        }

        let tree_leaves = snapshot.pane_tree.leaves();
        let pane_ids = panes.keys().copied().collect::<Vec<_>>();
        if tree_leaves.len() != pane_ids.len()
            || !tree_leaves
                .iter()
                .all(|pane_id| panes.contains_key(pane_id))
        {
            return Err(WorkspaceRestoreError::PaneTreeMismatch);
        }
        if !pane_ids
            .iter()
            .all(|pane_id| snapshot.pane_tree.contains_pane(*pane_id))
        {
            return Err(WorkspaceRestoreError::PaneTreeMismatch);
        }

        let max_buffer_id = buffers.keys().map(|id| id.raw()).max().unwrap_or(0);
        let max_pane_id = panes.keys().map(|id| id.raw()).max().unwrap_or(0);
        let max_job_id = jobs.entries().map(|job| job.id().raw()).max().unwrap_or(0);
        let ids = IdAllocator::from_next_ids(
            max_buffer_id + 1,
            max_pane_id + 1,
            snapshot.workspace_id.raw() + 1,
            max_job_id + 1,
        );

        Ok(Self {
            id: snapshot.workspace_id,
            buffers,
            panes,
            pane_tree: snapshot.pane_tree,
            active_pane: snapshot.active_pane,
            minibuffer: snapshot.minibuffer,
            messages: snapshot.messages,
            commands: super::default_commands(),
            keymaps: super::default_keymaps(),
            jobs,
            capabilities: super::default_capabilities(),
            ids,
        })
    }
}

impl WorkspaceSnapshot {
    pub fn save_to_path(&self, path: impl AsRef<Path>) -> Result<(), WorkspaceRestoreError> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|error| WorkspaceRestoreError::Json(error.to_string()))?;
        fs::write(path, json).map_err(|error| WorkspaceRestoreError::Io(error.to_string()))
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, WorkspaceRestoreError> {
        let json = fs::read_to_string(path)
            .map_err(|error| WorkspaceRestoreError::Io(error.to_string()))?;
        serde_json::from_str(&json).map_err(|error| WorkspaceRestoreError::Json(error.to_string()))
    }
}

impl BufferSnapshot {
    fn from_buffer(buffer: &Buffer) -> Self {
        Self {
            id: buffer.id(),
            name: buffer.name().to_string(),
            content: buffer.content().clone(),
        }
    }

    fn into_buffer(self) -> Buffer {
        Buffer::new(self.id, self.name, self.content)
    }
}

impl PaneSnapshot {
    fn from_pane(pane: &Pane) -> Self {
        Self {
            id: pane.id(),
            buffer_id: pane.buffer_id(),
            viewport: pane.viewport(),
            presentation: pane.presentation(),
            selection: pane.selection().cloned(),
        }
    }

    fn into_pane(self) -> Pane {
        let mut pane = Pane::new(self.id, self.buffer_id);
        pane.set_viewport(self.viewport);
        pane.set_presentation(self.presentation);
        pane.set_selection(self.selection);
        pane
    }
}

impl JobSnapshot {
    fn from_job(job: &Job) -> Self {
        Self {
            id: job.id(),
            name: job.name().to_string(),
            status: job.status().clone(),
            owner: job.owner(),
            kind: job.kind().clone(),
        }
    }

    fn into_job(self) -> Job {
        let mut job = Job::with_kind(self.id, self.name, self.owner, self.kind);
        job.set_status(self.status);
        job
    }
}
