use super::Session;
use crate::kernel::{
    Buffer, BufferContent, BufferKind, JobKind, JobOwner, JobStatus, MessageLevel, MinibufferMode,
    PaneNode, PanePresentation, Selection, SplitAxis, TreeNode, Viewport,
};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionView {
    pub pane_tree: SessionPaneNode,
    pub jobs: Vec<SessionJobView>,
    pub selected_job: Option<SessionJobView>,
    pub selected_item: Option<SessionSelectedItemView>,
    pub messages: Vec<SessionMessageView>,
    pub minibuffer_mode: MinibufferMode,
    pub minibuffer_text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionMessageView {
    pub level: MessageLevel,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionJobView {
    pub id: u64,
    pub name: String,
    pub status: SessionJobStatusView,
    pub owner: Option<SessionJobOwnerView>,
    pub kind: SessionJobKindView,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionSelectedItemView {
    Record {
        row: usize,
        value: Value,
    },
    TreeNode {
        id: String,
        label: String,
        linked_buffer_id: Option<u64>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionJobStatusView {
    Pending,
    Running,
    Succeeded,
    Failed(String),
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionJobOwnerView {
    Workspace(u64),
    Buffer(u64),
    Pane(u64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionJobKindView {
    Generic,
    PackageInvoke {
        package: String,
        command: String,
        output_buffer_id: u64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionPaneNode {
    Leaf(SessionPaneView),
    Split {
        axis: SplitAxis,
        ratio_percent: u8,
        first: Box<SessionPaneNode>,
        second: Box<SessionPaneNode>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionPaneView {
    pub buffer_name: String,
    pub buffer_id: u64,
    pub buffer_kind: BufferKind,
    pub pane_id: u64,
    pub viewport: Viewport,
    pub presentation: PanePresentation,
    pub selection: Option<Selection>,
    pub content: SessionPaneContentView,
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionPaneContentView {
    Text(Vec<String>),
    Records(Vec<Value>),
    Tree(Vec<SessionTreeNodeView>),
    Terminal {
        transcript: Vec<String>,
    },
    Browser {
        url: Option<String>,
        title: Option<String>,
    },
    Media {
        source: Option<String>,
    },
    Canvas {
        name: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionTreeNodeView {
    pub id: String,
    pub label: String,
    pub children: Vec<SessionTreeNodeView>,
}

impl Session {
    pub fn view(&self) -> SessionView {
        SessionView {
            pane_tree: self.build_pane_view(self.workspace.pane_tree().root()),
            jobs: self
                .workspace
                .jobs()
                .entries()
                .map(SessionJobView::from)
                .collect(),
            selected_job: self
                .workspace
                .selected_job_id_from_jobs_buffer()
                .and_then(|job_id| self.workspace.jobs().get(job_id))
                .map(SessionJobView::from),
            selected_item: selected_item_view(self.workspace()),
            messages: self
                .workspace
                .messages()
                .entries()
                .iter()
                .map(|message| SessionMessageView {
                    level: message.level(),
                    text: message.text().to_string(),
                })
                .collect(),
            minibuffer_mode: self.minibuffer().mode(),
            minibuffer_text: self.minibuffer().input().to_string(),
        }
    }

    fn build_pane_view(&self, node: &PaneNode) -> SessionPaneNode {
        match node {
            PaneNode::Leaf(pane_id) => {
                let pane = self
                    .workspace
                    .pane(*pane_id)
                    .expect("pane tree leaf should reference a live pane");
                let buffer = self
                    .workspace
                    .buffer(pane.buffer_id())
                    .expect("pane should reference a live buffer");
                SessionPaneNode::Leaf(SessionPaneView {
                    buffer_name: buffer.name().to_string(),
                    buffer_id: buffer.id().raw(),
                    buffer_kind: buffer.kind(),
                    pane_id: pane_id.raw(),
                    viewport: pane.viewport(),
                    presentation: pane.presentation(),
                    selection: pane.selection().cloned(),
                    content: buffer_content_view(buffer),
                    active: *pane_id == self.workspace.active_pane_id(),
                })
            }
            PaneNode::Split {
                axis,
                first,
                second,
                ratio_percent,
            } => SessionPaneNode::Split {
                axis: *axis,
                ratio_percent: *ratio_percent,
                first: Box::new(self.build_pane_view(first)),
                second: Box::new(self.build_pane_view(second)),
            },
        }
    }
}

impl From<&crate::kernel::Job> for SessionJobView {
    fn from(job: &crate::kernel::Job) -> Self {
        Self {
            id: job.id().raw(),
            name: job.name().to_string(),
            status: match job.status() {
                JobStatus::Pending => SessionJobStatusView::Pending,
                JobStatus::Running => SessionJobStatusView::Running,
                JobStatus::Succeeded => SessionJobStatusView::Succeeded,
                JobStatus::Failed(message) => SessionJobStatusView::Failed(message.clone()),
                JobStatus::Cancelled => SessionJobStatusView::Cancelled,
            },
            owner: job.owner().map(|owner| match owner {
                JobOwner::Workspace(id) => SessionJobOwnerView::Workspace(id.raw()),
                JobOwner::Buffer(id) => SessionJobOwnerView::Buffer(id.raw()),
                JobOwner::Pane(id) => SessionJobOwnerView::Pane(id.raw()),
            }),
            kind: match job.kind() {
                JobKind::Generic => SessionJobKindView::Generic,
                JobKind::PackageInvoke {
                    package,
                    command,
                    output_buffer_id,
                } => SessionJobKindView::PackageInvoke {
                    package: package.clone(),
                    command: command.clone(),
                    output_buffer_id: output_buffer_id.raw(),
                },
            },
        }
    }
}

fn buffer_content_view(buffer: &Buffer) -> SessionPaneContentView {
    match buffer.content() {
        BufferContent::Text(content) => SessionPaneContentView::Text(
            content
                .lines()
                .iter()
                .map(|line| line.text().to_string())
                .collect(),
        ),
        BufferContent::Records(content) => {
            SessionPaneContentView::Records(content.records().to_vec())
        }
        BufferContent::Tree(content) => {
            SessionPaneContentView::Tree(content.roots().iter().map(tree_node_view).collect())
        }
        BufferContent::Terminal(content) => SessionPaneContentView::Terminal {
            transcript: content
                .transcript()
                .lines()
                .iter()
                .map(|line| line.text().to_string())
                .collect(),
        },
        BufferContent::Browser(content) => SessionPaneContentView::Browser {
            url: content.url().map(ToOwned::to_owned),
            title: content.title().map(ToOwned::to_owned),
        },
        BufferContent::Media(content) => SessionPaneContentView::Media {
            source: content.source().map(ToOwned::to_owned),
        },
        BufferContent::Canvas(content) => SessionPaneContentView::Canvas {
            name: content.name().map(ToOwned::to_owned),
        },
    }
}

fn tree_node_view(node: &TreeNode) -> SessionTreeNodeView {
    SessionTreeNodeView {
        id: node.id().to_string(),
        label: node.label().to_string(),
        children: node.children().iter().map(tree_node_view).collect(),
    }
}

fn selected_item_view(workspace: &crate::kernel::Workspace) -> Option<SessionSelectedItemView> {
    if let Some(record) = workspace.current_record() {
        return workspace
            .selected_record_row()
            .map(|row| SessionSelectedItemView::Record {
                row,
                value: record.clone(),
            });
    }

    let tree_id = workspace.selected_tree_node_id()?;
    let node = workspace.current_tree_node()?;
    Some(SessionSelectedItemView::TreeNode {
        id: tree_id,
        label: node.label().to_string(),
        linked_buffer_id: node.linked_buffer_id().map(|id| id.raw()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::{
        BrowserContent, BufferContent, JobKind, JobOwner, JobStatus, PanePresentation, Selection,
        SplitAxis, SurfaceSelection, TreeContent, TreeNode, Viewport,
    };

    #[test]
    fn view_preserves_split_pane_structure() {
        let mut session = Session::new();
        session.dispatch_command("pane.split.vertical");
        session.dispatch_command("pane.split.horizontal");

        let view = session.view();

        match view.pane_tree {
            SessionPaneNode::Split {
                axis: SplitAxis::Vertical,
                ratio_percent: 50,
                first,
                second,
            } => {
                assert!(matches!(*first, SessionPaneNode::Leaf(_)));
                assert!(matches!(
                    *second,
                    SessionPaneNode::Split {
                        axis: SplitAxis::Horizontal,
                        ratio_percent: 50,
                        ..
                    }
                ));
            }
            other => panic!("unexpected pane tree: {other:?}"),
        }
    }

    #[test]
    fn view_exposes_workspace_message_log() {
        let mut session = Session::new();
        session
            .workspace_mut()
            .messages_mut()
            .info("startup complete");
        session
            .workspace_mut()
            .messages_mut()
            .warn("package failed");

        let view = session.view();

        assert_eq!(
            view.messages,
            vec![
                SessionMessageView {
                    level: MessageLevel::Info,
                    text: "startup complete".to_string(),
                },
                SessionMessageView {
                    level: MessageLevel::Warning,
                    text: "package failed".to_string(),
                },
            ]
        );
    }

    #[test]
    fn view_exposes_workspace_jobs() {
        let mut session = Session::new();
        let output_buffer = session.workspace_mut().create_records_buffer("results");
        let workspace_id = session.workspace().id();
        let generic = session
            .workspace_mut()
            .create_job("index workspace", Some(JobOwner::Workspace(workspace_id)));
        assert!(
            session
                .workspace_mut()
                .set_job_status(generic, JobStatus::Running)
        );
        let package = session.workspace_mut().create_job_with_kind(
            "package echo default",
            Some(JobOwner::Workspace(workspace_id)),
            JobKind::PackageInvoke {
                package: "echo".to_string(),
                command: "default".to_string(),
                output_buffer_id: output_buffer,
            },
        );
        assert!(
            session
                .workspace_mut()
                .set_job_status(package, JobStatus::Succeeded)
        );

        let view = session.view();

        assert_eq!(
            view.jobs,
            vec![
                SessionJobView {
                    id: generic.raw(),
                    name: "index workspace".to_string(),
                    status: SessionJobStatusView::Running,
                    owner: Some(SessionJobOwnerView::Workspace(workspace_id.raw())),
                    kind: SessionJobKindView::Generic,
                },
                SessionJobView {
                    id: package.raw(),
                    name: "package echo default".to_string(),
                    status: SessionJobStatusView::Succeeded,
                    owner: Some(SessionJobOwnerView::Workspace(workspace_id.raw())),
                    kind: SessionJobKindView::PackageInvoke {
                        package: "echo".to_string(),
                        command: "default".to_string(),
                        output_buffer_id: output_buffer.raw(),
                    },
                },
            ]
        );
        assert_eq!(view.selected_job, None);
        assert_eq!(view.selected_item, None);
    }

    #[test]
    fn view_preserves_jobs_after_workspace_restore() {
        let mut session = Session::new();
        let output_buffer = session.workspace_mut().create_records_buffer("results");
        let workspace_id = session.workspace().id();
        let job_id = session.workspace_mut().create_job_with_kind(
            "package echo default",
            Some(JobOwner::Workspace(workspace_id)),
            JobKind::PackageInvoke {
                package: "echo".to_string(),
                command: "default".to_string(),
                output_buffer_id: output_buffer,
            },
        );
        assert!(
            session
                .workspace_mut()
                .set_job_status(job_id, JobStatus::Succeeded)
        );

        let snapshot = session.workspace().snapshot();
        let restored =
            crate::kernel::Workspace::from_snapshot(snapshot).expect("snapshot should restore");
        session.replace_workspace(restored);

        let view = session.view();

        assert_eq!(view.jobs.len(), 1);
        assert_eq!(view.jobs[0].id, job_id.raw());
        assert_eq!(view.jobs[0].name, "package echo default");
        assert_eq!(view.jobs[0].status, SessionJobStatusView::Succeeded);
        assert_eq!(view.selected_job, None);
        assert_eq!(view.selected_item, None);
    }

    #[test]
    fn view_surfaces_selected_job_from_jobs_buffer_row() {
        let mut session = Session::new();
        let output_buffer = session.workspace_mut().create_records_buffer("results");
        let workspace_id = session.workspace().id();
        let job_id = session.workspace_mut().create_job_with_kind(
            "package echo default",
            Some(JobOwner::Workspace(workspace_id)),
            JobKind::PackageInvoke {
                package: "echo".to_string(),
                command: "default".to_string(),
                output_buffer_id: output_buffer,
            },
        );
        assert!(
            session
                .workspace_mut()
                .set_job_status(job_id, JobStatus::Succeeded)
        );

        let job_records = session.workspace().job_records();
        let jobs_buffer = session.workspace_mut().create_buffer(
            "*jobs*",
            BufferContent::Records(crate::kernel::RecordsContent::new(job_records)),
        );
        assert!(session.workspace_mut().focus_buffer(jobs_buffer));
        assert!(session.workspace_mut().select_record_row(0));

        let view = session.view();

        assert_eq!(
            view.selected_job,
            Some(SessionJobView {
                id: job_id.raw(),
                name: "package echo default".to_string(),
                status: SessionJobStatusView::Succeeded,
                owner: Some(SessionJobOwnerView::Workspace(workspace_id.raw())),
                kind: SessionJobKindView::PackageInvoke {
                    package: "echo".to_string(),
                    command: "default".to_string(),
                    output_buffer_id: output_buffer.raw(),
                },
            })
        );
        assert_eq!(
            view.selected_item,
            Some(SessionSelectedItemView::Record {
                row: 0,
                value: serde_json::json!({
                    "id": job_id.raw(),
                    "name": "package echo default",
                    "status": "succeeded",
                    "owner_kind": "workspace",
                    "owner_id": workspace_id.raw(),
                    "kind": "package_invoke",
                    "package": "echo",
                    "command": "default",
                    "output_buffer_id": output_buffer.raw(),
                    "output_buffer_name": "*pkg:echo default*",
                    "has_output": true,
                    "summary": format!(
                        "package echo default [succeeded] echo default -> buffer {}",
                        output_buffer.raw()
                    ),
                }),
            })
        );
    }

    #[test]
    fn view_surfaces_selected_tree_node() {
        let mut session = Session::new();

        let mut root = TreeNode::new("root", "Root");
        root.push_child(TreeNode::new("child", "Child"));
        let tree = session
            .workspace_mut()
            .create_buffer("tree", BufferContent::Tree(TreeContent::new(vec![root])));
        assert!(session.workspace_mut().focus_buffer(tree));
        assert!(session.workspace_mut().select_tree_node("child"));

        let view = session.view();

        assert_eq!(
            view.selected_item,
            Some(SessionSelectedItemView::TreeNode {
                id: "child".to_string(),
                label: "Child".to_string(),
                linked_buffer_id: None,
            })
        );
    }

    #[test]
    fn view_preserves_records_and_browser_content_kinds() {
        let mut session = Session::new();

        let records = session.workspace_mut().create_records_buffer("results");
        assert!(
            session
                .workspace_mut()
                .push_record_to_buffer(records, serde_json::json!({"ok": true}))
        );
        assert!(session.workspace_mut().focus_buffer(records));

        let view = session.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };
        assert_eq!(pane.buffer_name, "results");
        assert_eq!(pane.buffer_kind, BufferKind::Records);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Records(vec![serde_json::json!({"ok": true})])
        );

        let browser = session.workspace_mut().create_buffer(
            "browser",
            BufferContent::Browser(BrowserContent::new(
                Some("https://example.com".to_string()),
                Some("Example".to_string()),
            )),
        );
        assert!(session.workspace_mut().focus_buffer(browser));

        let view = session.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };
        assert_eq!(pane.buffer_name, "browser");
        assert_eq!(pane.buffer_kind, BufferKind::Browser);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Browser {
                url: Some("https://example.com".to_string()),
                title: Some("Example".to_string()),
            }
        );
    }

    #[test]
    fn view_preserves_tree_hierarchy() {
        let mut session = Session::new();

        let mut root = TreeNode::new("root", "Root");
        root.push_child(TreeNode::new("child", "Child"));
        let tree = session
            .workspace_mut()
            .create_buffer("tree", BufferContent::Tree(TreeContent::new(vec![root])));
        assert!(session.workspace_mut().focus_buffer(tree));

        let view = session.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };
        assert_eq!(pane.buffer_name, "tree");
        assert_eq!(pane.buffer_kind, BufferKind::Tree);
        assert_eq!(
            pane.content,
            SessionPaneContentView::Tree(vec![SessionTreeNodeView {
                id: "root".to_string(),
                label: "Root".to_string(),
                children: vec![SessionTreeNodeView {
                    id: "child".to_string(),
                    label: "Child".to_string(),
                    children: Vec::new(),
                }],
            }])
        );
    }

    #[test]
    fn view_preserves_pane_view_state() {
        let mut session = Session::new();
        let pane_id = session.workspace().active_pane_id();
        let pane = session
            .workspace_mut()
            .pane_mut(pane_id)
            .expect("active pane should exist");
        pane.set_viewport(Viewport::new(4, 8));
        pane.set_presentation(PanePresentation::Preview);

        let view = session.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(pane.viewport, Viewport::new(4, 8));
        assert_eq!(pane.presentation, PanePresentation::Preview);
    }

    #[test]
    fn view_preserves_pane_selection() {
        let mut session = Session::new();
        let pane_id = session.workspace().active_pane_id();
        session
            .workspace_mut()
            .pane_mut(pane_id)
            .expect("active pane should exist")
            .set_selection(Some(Selection::Surface(SurfaceSelection::new("hotspot"))));

        let view = session.view();
        let SessionPaneNode::Leaf(pane) = view.pane_tree else {
            panic!("expected leaf pane view");
        };

        assert_eq!(
            pane.selection,
            Some(Selection::Surface(SurfaceSelection::new("hotspot")))
        );
    }
}
