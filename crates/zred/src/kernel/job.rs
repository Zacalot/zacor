use crate::kernel::ids::{BufferId, JobId, PaneId, WorkspaceId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JobRegistry {
    jobs: BTreeMap<JobId, Job>,
}

impl JobRegistry {
    pub fn new() -> Self {
        Self {
            jobs: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, job: Job) -> Option<Job> {
        self.jobs.insert(job.id(), job)
    }

    pub fn get(&self, id: JobId) -> Option<&Job> {
        self.jobs.get(&id)
    }

    #[allow(dead_code)]
    pub fn get_mut(&mut self, id: JobId) -> Option<&mut Job> {
        self.jobs.get_mut(&id)
    }

    pub fn cancel(&mut self, id: JobId) -> bool {
        let Some(job) = self.jobs.get_mut(&id) else {
            return false;
        };

        job.set_status(JobStatus::Cancelled);
        true
    }

    #[allow(dead_code)]
    pub fn entries(&self) -> impl Iterator<Item = &Job> {
        self.jobs.values()
    }
}

impl Default for JobRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Job {
    id: JobId,
    name: String,
    status: JobStatus,
    owner: Option<JobOwner>,
    kind: JobKind,
}

impl Job {
    pub fn new(id: JobId, name: impl Into<String>, owner: Option<JobOwner>) -> Self {
        Self::with_kind(id, name, owner, JobKind::Generic)
    }

    pub fn with_kind(
        id: JobId,
        name: impl Into<String>,
        owner: Option<JobOwner>,
        kind: JobKind,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            status: JobStatus::Pending,
            owner,
            kind,
        }
    }

    pub fn id(&self) -> JobId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn status(&self) -> &JobStatus {
        &self.status
    }

    pub fn set_status(&mut self, status: JobStatus) {
        self.status = status;
    }

    pub fn owner(&self) -> Option<JobOwner> {
        self.owner
    }

    pub fn kind(&self) -> &JobKind {
        &self.kind
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum JobStatus {
    Pending,
    Running,
    Succeeded,
    Failed(String),
    Cancelled,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum JobOwner {
    Workspace(WorkspaceId),
    Buffer(BufferId),
    Pane(PaneId),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum JobKind {
    Generic,
    PackageInvoke {
        package: String,
        command: String,
        output_buffer_id: BufferId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_job_cancel_fails_softly() {
        let mut registry = JobRegistry::new();

        assert!(!registry.cancel(JobId::new(9)));
    }

    #[test]
    fn cancel_marks_existing_job() {
        let mut registry = JobRegistry::new();
        registry.insert(Job::new(JobId::new(1), "grep", None));

        assert!(registry.cancel(JobId::new(1)));
        assert_eq!(
            registry.get(JobId::new(1)).unwrap().status(),
            &JobStatus::Cancelled
        );
    }
}
