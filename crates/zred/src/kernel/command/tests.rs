use super::metadata::{Command, CommandScope, CommandSpec};
use super::registry::CommandRegistry;
use super::types::{
    CommandData, CommandEffect, CommandInvocation, CommandRequest, PackageInvocationRequest,
};
use crate::kernel::{BufferContent, BufferId, JobKind, Workspace};

#[test]
fn registry_indexes_commands_by_name() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::new("quit", "Quit zred", CommandScope::Global));

    assert!(registry.contains("quit"));
    assert_eq!(registry.get("quit").unwrap().summary(), "Quit zred");
}

#[test]
fn dispatch_creates_and_focuses_new_buffer() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "buffer.new",
        "Create a text buffer",
        CommandScope::Workspace,
        CommandSpec::buffer_new(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "buffer.new notes");

    assert_eq!(workspace.buffer_count(), 2);
    assert_eq!(workspace.current_buffer().name(), "notes");
    assert_eq!(
        result.data(),
        Some(&CommandData::BufferCreated {
            buffer_id: workspace.current_buffer().id()
        })
    );
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus("Created notes".to_string())]
    );
}

#[test]
fn dispatch_buffer_describe_reports_text_buffer_summary() {
    let registry = CommandRegistry::new();
    let workspace = &mut Workspace::new();

    let result = registry.dispatch_invocation(workspace, CommandInvocation::BufferDescribe);

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "Buffer 1: text (1 lines)".to_string()
        )]
    );
}

#[test]
fn dispatch_workspace_save_returns_runtime_effect_and_status() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::WorkspaceSave {
            path: "state.json".to_string(),
        },
    );

    assert_eq!(
        result.effects(),
        &[
            CommandEffect::SaveWorkspace("state.json".to_string()),
            CommandEffect::SetStatus("Saved workspace to state.json".to_string()),
        ]
    );
}

#[test]
fn dispatch_workspace_load_returns_runtime_effect_and_status() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::WorkspaceLoad {
            path: "state.json".to_string(),
        },
    );

    assert_eq!(
        result.effects(),
        &[
            CommandEffect::LoadWorkspace("state.json".to_string()),
            CommandEffect::SetStatus("Loaded workspace from state.json".to_string()),
        ]
    );
}

#[test]
fn dispatch_window_new_returns_frontend_effect_and_status() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();

    let result = registry.dispatch_invocation(&mut workspace, CommandInvocation::NewWindow);

    assert_eq!(
        result.effects(),
        &[
            CommandEffect::NewWindow,
            CommandEffect::SetStatus("Opened a new window".to_string()),
        ]
    );
}

#[test]
fn dispatch_terminal_open_creates_and_focuses_terminal_buffer() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "terminal.open",
        "Open a terminal buffer",
        CommandScope::Workspace,
        CommandSpec::terminal_open(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "terminal.open shell");

    assert_eq!(workspace.buffer_count(), 2);
    assert_eq!(workspace.current_buffer().name(), "*terminal:shell*");
    assert!(matches!(
        workspace.current_buffer().content(),
        BufferContent::Terminal(_)
    ));
    assert_eq!(
        result.data(),
        Some(&CommandData::BufferCreated {
            buffer_id: workspace.current_buffer().id()
        })
    );
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "Opened terminal shell".to_string()
        )]
    );
}

#[test]
fn dispatch_terminal_append_updates_terminal_buffer() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_buffer(
        "terminal",
        BufferContent::Terminal(crate::kernel::TerminalContent::default()),
    );

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::TerminalAppend {
            buffer_id: Some(buffer_id),
            text: "hello\nworld".to_string(),
        },
    );

    assert!(matches!(
        workspace.buffer(buffer_id).unwrap().content(),
        BufferContent::Terminal(content)
            if content
                .transcript()
                .lines()
                .iter()
                .map(|line| line.text())
                .collect::<Vec<_>>()
                == vec!["hello", "world"]
    ));
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Appended terminal output for {buffer_id}"
        ))]
    );
}

#[test]
fn dispatch_buffer_describe_reports_terminal_buffer_summary() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_buffer(
        "terminal",
        BufferContent::Terminal(crate::kernel::TerminalContent::default()),
    );
    assert!(workspace.append_to_terminal_buffer(buffer_id, "hello\nworld"));
    assert!(workspace.focus_buffer(buffer_id));

    let result = registry.dispatch_invocation(&mut workspace, CommandInvocation::BufferDescribe);

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Buffer {buffer_id}: terminal (2 lines)"
        ))]
    );
}

#[test]
fn dispatch_buffer_describe_reports_records_buffer_summary() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_buffer(
        "records",
        BufferContent::Records(crate::kernel::RecordsContent::new(vec![
            serde_json::json!({"value": 1}),
            serde_json::json!({"value": 2}),
        ])),
    );
    assert!(workspace.focus_buffer(buffer_id));

    let result = registry.dispatch_invocation(&mut workspace, CommandInvocation::BufferDescribe);

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Buffer {buffer_id}: records (2 records)"
        ))]
    );
}

#[test]
fn dispatch_buffer_describe_reports_tree_buffer_summary() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_buffer(
        "tree",
        BufferContent::Tree(crate::kernel::TreeContent::new(vec![
            crate::kernel::TreeNode::new("root-1", "Root 1"),
            crate::kernel::TreeNode::new("root-2", "Root 2"),
        ])),
    );
    assert!(workspace.focus_buffer(buffer_id));

    let result = registry.dispatch_invocation(&mut workspace, CommandInvocation::BufferDescribe);

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Buffer {buffer_id}: tree (2 roots)"
        ))]
    );
}

#[test]
fn dispatch_browser_open_creates_and_focuses_browser_buffer() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "browser.open",
        "Open a browser buffer",
        CommandScope::Workspace,
        CommandSpec::browser_open(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "browser.open https://example.com");

    assert_eq!(workspace.buffer_count(), 2);
    assert_eq!(
        workspace.current_buffer().name(),
        "*browser:https://example.com*"
    );
    assert!(matches!(
        workspace.current_buffer().content(),
        BufferContent::Browser(content)
            if content.url() == Some("https://example.com") && content.title().is_none()
    ));
    assert_eq!(
        result.data(),
        Some(&CommandData::BufferCreated {
            buffer_id: workspace.current_buffer().id()
        })
    );
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "Opened https://example.com".to_string()
        )]
    );
}

#[test]
fn dispatch_browser_set_url_updates_browser_buffer() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_buffer(
        "browser",
        BufferContent::Browser(crate::kernel::BrowserContent::new(
            Some("https://before.example".to_string()),
            None,
        )),
    );

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::BrowserSetUrl {
            buffer_id: Some(buffer_id),
            url: "https://after.example".to_string(),
        },
    );

    assert!(matches!(
        workspace.buffer(buffer_id).unwrap().content(),
        BufferContent::Browser(content)
            if content.url() == Some("https://after.example") && content.title().is_none()
    ));
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Set browser url for {buffer_id}"
        ))]
    );
}

#[test]
fn dispatch_browser_set_title_updates_browser_buffer() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_buffer(
        "browser",
        BufferContent::Browser(crate::kernel::BrowserContent::new(
            Some("https://example.com".to_string()),
            None,
        )),
    );

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::BrowserSetTitle {
            buffer_id: Some(buffer_id),
            title: "Example Domain".to_string(),
        },
    );

    assert!(matches!(
        workspace.buffer(buffer_id).unwrap().content(),
        BufferContent::Browser(content)
            if content.url() == Some("https://example.com")
                && content.title() == Some("Example Domain")
    ));
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Set browser title for {buffer_id}"
        ))]
    );
}

#[test]
fn dispatch_buffer_describe_reports_browser_buffer_summary() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_buffer(
        "browser",
        BufferContent::Browser(crate::kernel::BrowserContent::new(
            Some("https://example.com".to_string()),
            Some("Example Domain".to_string()),
        )),
    );
    assert!(workspace.focus_buffer(buffer_id));

    let result = registry.dispatch_invocation(&mut workspace, CommandInvocation::BufferDescribe);

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Buffer {buffer_id}: browser https://example.com (Example Domain)"
        ))]
    );
}

#[test]
fn dispatch_media_open_creates_and_focuses_media_buffer() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "media.open",
        "Open a media buffer",
        CommandScope::Workspace,
        CommandSpec::media_open(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "media.open ./clip.mp4");

    assert_eq!(workspace.buffer_count(), 2);
    assert_eq!(workspace.current_buffer().name(), "*media:./clip.mp4*");
    assert!(matches!(
        workspace.current_buffer().content(),
        BufferContent::Media(content) if content.source() == Some("./clip.mp4")
    ));
    assert_eq!(
        result.data(),
        Some(&CommandData::BufferCreated {
            buffer_id: workspace.current_buffer().id()
        })
    );
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus("Opened ./clip.mp4".to_string())]
    );
}

#[test]
fn dispatch_media_set_source_updates_media_buffer() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_buffer(
        "media",
        BufferContent::Media(crate::kernel::MediaContent::new(Some(
            "./before.mp4".to_string(),
        ))),
    );

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::MediaSetSource {
            buffer_id: Some(buffer_id),
            source: "./after.mp4".to_string(),
        },
    );

    assert!(matches!(
        workspace.buffer(buffer_id).unwrap().content(),
        BufferContent::Media(content) if content.source() == Some("./after.mp4")
    ));
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Set media source for {buffer_id}"
        ))]
    );
}

#[test]
fn dispatch_buffer_describe_reports_media_buffer_summary() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_buffer(
        "media",
        BufferContent::Media(crate::kernel::MediaContent::new(Some(
            "./clip.mp4".to_string(),
        ))),
    );
    assert!(workspace.focus_buffer(buffer_id));

    let result = registry.dispatch_invocation(&mut workspace, CommandInvocation::BufferDescribe);

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Buffer {buffer_id}: media ./clip.mp4"
        ))]
    );
}

#[test]
fn dispatch_canvas_open_creates_and_focuses_canvas_buffer() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "canvas.open",
        "Open a canvas buffer",
        CommandScope::Workspace,
        CommandSpec::canvas_open(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "canvas.open playground");

    assert_eq!(workspace.buffer_count(), 2);
    assert_eq!(workspace.current_buffer().name(), "*canvas:playground*");
    assert!(matches!(
        workspace.current_buffer().content(),
        BufferContent::Canvas(content) if content.name() == Some("playground")
    ));
    assert_eq!(
        result.data(),
        Some(&CommandData::BufferCreated {
            buffer_id: workspace.current_buffer().id()
        })
    );
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus("Opened playground".to_string())]
    );
}

#[test]
fn dispatch_canvas_set_name_updates_canvas_buffer() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_buffer(
        "canvas",
        BufferContent::Canvas(crate::kernel::CanvasContent::new(Some(
            "before".to_string(),
        ))),
    );

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::CanvasSetName {
            buffer_id: Some(buffer_id),
            name: "after".to_string(),
        },
    );

    assert!(matches!(
        workspace.buffer(buffer_id).unwrap().content(),
        BufferContent::Canvas(content) if content.name() == Some("after")
    ));
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Set canvas name for {buffer_id}"
        ))]
    );
}

#[test]
fn dispatch_buffer_describe_reports_canvas_buffer_summary() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_buffer(
        "canvas",
        BufferContent::Canvas(crate::kernel::CanvasContent::new(Some(
            "playground".to_string(),
        ))),
    );
    assert!(workspace.focus_buffer(buffer_id));

    let result = registry.dispatch_invocation(&mut workspace, CommandInvocation::BufferDescribe);

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Buffer {buffer_id}: canvas playground"
        ))]
    );
}

#[test]
fn dispatch_quit_returns_quit_outcome() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "quit",
        "Quit zred",
        CommandScope::Global,
        CommandSpec::quit(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "quit");

    assert_eq!(result.effects(), &[CommandEffect::Quit]);
}

#[test]
fn dispatch_job_list_reports_jobs_summary() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let job_id = workspace.create_job("index workspace", None);
    assert!(workspace.set_job_status(job_id, crate::kernel::JobStatus::Running));

    let result = registry.dispatch_invocation(&mut workspace, CommandInvocation::JobList);

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "Jobs: 1:index workspace [running]".to_string()
        )]
    );
}

#[test]
fn dispatch_job_open_creates_records_buffer_snapshot() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let output_buffer = workspace.create_records_buffer("results");
    let job_id = workspace.create_job_with_kind(
        "package echo default",
        None,
        JobKind::PackageInvoke {
            package: "echo".to_string(),
            command: "default".to_string(),
            output_buffer_id: output_buffer,
        },
    );
    assert!(workspace.set_job_status(job_id, crate::kernel::JobStatus::Succeeded));

    let result = registry.dispatch_invocation(&mut workspace, CommandInvocation::JobOpen);

    assert_eq!(workspace.current_buffer().name(), "*jobs*");
    assert!(matches!(
        workspace.current_buffer().content(),
        BufferContent::Records(content)
            if content.records()
                == &[serde_json::json!({
                    "id": job_id.raw(),
                    "name": "package echo default",
                    "status": "succeeded",
                    "owner_kind": serde_json::Value::Null,
                    "owner_id": serde_json::Value::Null,
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
                })]
    ));
    assert_eq!(
        result.data(),
        Some(&CommandData::BufferCreated {
            buffer_id: workspace.current_buffer().id()
        })
    );
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "Opened jobs buffer (1)".to_string()
        )]
    );
}

#[test]
fn dispatch_job_open_reuses_existing_jobs_buffer() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let first = registry.dispatch_invocation(&mut workspace, CommandInvocation::JobOpen);
    let first_buffer_id = workspace.current_buffer().id();

    let output_buffer = workspace.create_records_buffer("results");
    let job_id = workspace.create_job_with_kind(
        "package echo default",
        None,
        JobKind::PackageInvoke {
            package: "echo".to_string(),
            command: "default".to_string(),
            output_buffer_id: output_buffer,
        },
    );
    assert!(workspace.set_job_status(job_id, crate::kernel::JobStatus::Succeeded));

    let second = registry.dispatch_invocation(&mut workspace, CommandInvocation::JobOpen);

    assert_eq!(workspace.current_buffer().id(), first_buffer_id);
    assert_eq!(workspace.buffer_count(), 3);
    assert!(matches!(
        workspace.current_buffer().content(),
        BufferContent::Records(content)
            if content.records()
                == &[serde_json::json!({
                    "id": job_id.raw(),
                    "name": "package echo default",
                    "status": "succeeded",
                    "owner_kind": serde_json::Value::Null,
                    "owner_id": serde_json::Value::Null,
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
                })]
    ));
    assert_eq!(
        first.effects(),
        &[CommandEffect::SetStatus(
            "Opened jobs buffer (0)".to_string()
        )]
    );
    assert_eq!(second.data(), None);
    assert_eq!(
        second.effects(),
        &[CommandEffect::SetStatus(
            "Refreshed jobs buffer (1)".to_string()
        )]
    );
}

#[test]
fn dispatch_job_describe_reports_job_details() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let output_buffer = workspace.create_records_buffer("results");
    let job_id = workspace.create_job_with_kind(
        "package echo default",
        None,
        JobKind::PackageInvoke {
            package: "echo".to_string(),
            command: "default".to_string(),
            output_buffer_id: output_buffer,
        },
    );
    assert!(workspace.set_job_status(job_id, crate::kernel::JobStatus::Succeeded));

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::JobDescribe {
            job_id: Some(job_id),
        },
    );

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Job {job_id}: package echo default [succeeded] echo default -> buffer {output_buffer}"
        ))]
    );
}

#[test]
fn dispatch_job_focus_output_targets_package_buffer() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let output_buffer = workspace.create_records_buffer("results");
    let job_id = workspace.create_job_with_kind(
        "package echo default",
        None,
        JobKind::PackageInvoke {
            package: "echo".to_string(),
            command: "default".to_string(),
            output_buffer_id: output_buffer,
        },
    );

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::JobFocusOutput {
            job_id: Some(job_id),
        },
    );

    assert_eq!(workspace.current_buffer().id(), output_buffer);
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Focused output buffer {output_buffer} for job {job_id} (echo default)"
        ))]
    );
}

#[test]
fn dispatch_job_cancel_updates_job_status() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let job_id = workspace.create_job("index workspace", None);

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::JobCancel {
            job_id: Some(job_id),
        },
    );

    assert_eq!(
        workspace.jobs().get(job_id).unwrap().status(),
        &crate::kernel::JobStatus::Cancelled
    );
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!("Cancelled job {job_id}"))]
    );
}

#[test]
fn dispatch_job_describe_uses_selected_jobs_buffer_row() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let output_buffer = workspace.create_records_buffer("results");
    let job_id = workspace.create_job_with_kind(
        "package echo default",
        None,
        JobKind::PackageInvoke {
            package: "echo".to_string(),
            command: "default".to_string(),
            output_buffer_id: output_buffer,
        },
    );
    assert!(workspace.set_job_status(job_id, crate::kernel::JobStatus::Succeeded));
    let _ = registry.dispatch_invocation(&mut workspace, CommandInvocation::JobOpen);
    assert!(workspace.select_record_row(0));

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::JobDescribe { job_id: None },
    );

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Job {job_id}: package echo default [succeeded] echo default -> buffer {output_buffer}"
        ))]
    );
}

#[test]
fn dispatch_job_focus_output_uses_selected_jobs_buffer_row() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let output_buffer = workspace.create_records_buffer("results");
    let job_id = workspace.create_job_with_kind(
        "package echo default",
        None,
        JobKind::PackageInvoke {
            package: "echo".to_string(),
            command: "default".to_string(),
            output_buffer_id: output_buffer,
        },
    );
    let _ = registry.dispatch_invocation(&mut workspace, CommandInvocation::JobOpen);
    assert!(workspace.select_record_row(0));

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::JobFocusOutput { job_id: None },
    );

    assert_eq!(workspace.current_buffer().id(), output_buffer);
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Focused output buffer {output_buffer} for job {job_id} (echo default)"
        ))]
    );
}

#[test]
fn dispatch_job_cancel_uses_selected_jobs_buffer_row() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let job_id = workspace.create_job("index workspace", None);
    let _ = registry.dispatch_invocation(&mut workspace, CommandInvocation::JobOpen);
    assert!(workspace.select_record_row(0));

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::JobCancel { job_id: None },
    );

    assert_eq!(
        workspace.jobs().get(job_id).unwrap().status(),
        &crate::kernel::JobStatus::Cancelled
    );
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!("Cancelled job {job_id}"))]
    );
}

#[test]
fn dispatch_job_next_moves_selected_jobs_buffer_row() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    workspace.create_job("one", None);
    workspace.create_job("two", None);
    let _ = registry.dispatch_invocation(&mut workspace, CommandInvocation::JobOpen);

    let result = registry.dispatch_invocation(&mut workspace, CommandInvocation::JobNext);

    assert_eq!(
        workspace.selected_job_id_from_jobs_buffer(),
        Some(crate::kernel::JobId::new(2))
    );
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus("Selected job row 2".to_string())]
    );
}

#[test]
fn dispatch_job_previous_wraps_selected_jobs_buffer_row() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    workspace.create_job("one", None);
    workspace.create_job("two", None);
    let _ = registry.dispatch_invocation(&mut workspace, CommandInvocation::JobOpen);

    let result = registry.dispatch_invocation(&mut workspace, CommandInvocation::JobPrevious);

    assert_eq!(
        workspace.selected_job_id_from_jobs_buffer(),
        Some(crate::kernel::JobId::new(2))
    );
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus("Selected job row 2".to_string())]
    );
}

#[test]
fn dispatch_buffer_structured_next_moves_selected_records_row() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_records_buffer("results");
    assert!(workspace.push_record_to_buffer(buffer_id, serde_json::json!({"id": 1})));
    assert!(workspace.push_record_to_buffer(buffer_id, serde_json::json!({"id": 2})));
    assert!(workspace.focus_buffer(buffer_id));

    let result =
        registry.dispatch_invocation(&mut workspace, CommandInvocation::BufferStructuredNext);

    assert_eq!(workspace.selected_record_row(), Some(1));
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "Selected record row 2".to_string()
        )]
    );
}

#[test]
fn dispatch_buffer_structured_current_reports_selected_record() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_records_buffer("results");
    assert!(workspace.push_record_to_buffer(buffer_id, serde_json::json!({"id": 1, "ok": true})));
    assert!(workspace.focus_buffer(buffer_id));
    assert!(workspace.select_record_row(0));

    let result =
        registry.dispatch_invocation(&mut workspace, CommandInvocation::BufferStructuredCurrent);

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "Record: {\"id\":1,\"ok\":true}".to_string()
        )]
    );
}

#[test]
fn dispatch_buffer_structured_open_focuses_linked_record_buffer() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let target = workspace.create_text_buffer("target");
    let results = workspace.create_records_buffer("results");
    assert!(workspace.push_record_to_buffer(
        results,
        serde_json::json!({"buffer_id": target.raw(), "label": "target"})
    ));
    assert!(workspace.focus_buffer(results));
    assert!(workspace.select_record_row(0));

    let result =
        registry.dispatch_invocation(&mut workspace, CommandInvocation::BufferStructuredOpen);

    assert_eq!(workspace.current_buffer().id(), target);
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Focused linked buffer {target}"
        ))]
    );
}

#[test]
fn dispatch_buffer_structured_next_moves_selected_tree_node() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let mut root = crate::kernel::TreeNode::new("root", "Root");
    root.push_child(crate::kernel::TreeNode::new("child", "Child"));
    let buffer_id = workspace.create_buffer(
        "tree",
        BufferContent::Tree(crate::kernel::TreeContent::new(vec![root])),
    );
    assert!(workspace.focus_buffer(buffer_id));

    let result =
        registry.dispatch_invocation(&mut workspace, CommandInvocation::BufferStructuredNext);

    assert_eq!(workspace.selected_tree_node_id().as_deref(), Some("child"));
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "Selected tree node child".to_string()
        )]
    );
}

#[test]
fn dispatch_buffer_structured_current_reports_selected_tree_node() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let mut root = crate::kernel::TreeNode::new("root", "Root");
    root.push_child(crate::kernel::TreeNode::new("child", "Child"));
    let buffer_id = workspace.create_buffer(
        "tree",
        BufferContent::Tree(crate::kernel::TreeContent::new(vec![root])),
    );
    assert!(workspace.focus_buffer(buffer_id));
    assert!(workspace.select_tree_node("child"));

    let result =
        registry.dispatch_invocation(&mut workspace, CommandInvocation::BufferStructuredCurrent);

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "Tree node child: Child".to_string()
        )]
    );
}

#[test]
fn dispatch_buffer_structured_open_focuses_linked_tree_buffer() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let target = workspace.create_text_buffer("target");
    let mut root = crate::kernel::TreeNode::new("root", "Root");
    root.push_child(crate::kernel::TreeNode::with_linked_buffer(
        "child", "Child", target,
    ));
    let buffer_id = workspace.create_buffer(
        "tree",
        BufferContent::Tree(crate::kernel::TreeContent::new(vec![root])),
    );
    assert!(workspace.focus_buffer(buffer_id));
    assert!(workspace.select_tree_node("child"));

    let result =
        registry.dispatch_invocation(&mut workspace, CommandInvocation::BufferStructuredOpen);

    assert_eq!(workspace.current_buffer().id(), target);
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Focused linked buffer {target}"
        ))]
    );
}

#[test]
fn dispatch_help_lists_registered_commands() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "browser.open",
        "Open a browser buffer",
        CommandScope::Workspace,
        CommandSpec::browser_open(),
    ));
    registry.register(Command::with_spec(
        "buffer.describe",
        "Describe the active buffer",
        CommandScope::Workspace,
        CommandSpec::buffer_describe(),
    ));
    registry.register(Command::with_spec(
        "workspace.load",
        "Load workspace from a snapshot file",
        CommandScope::Workspace,
        CommandSpec::workspace_load(),
    ));
    registry.register(Command::with_spec(
        "workspace.save",
        "Save workspace to a snapshot file",
        CommandScope::Workspace,
        CommandSpec::workspace_save(),
    ));
    registry.register(Command::with_spec(
        "job.cancel",
        "Cancel a job",
        CommandScope::Workspace,
        CommandSpec::job_cancel(),
    ));
    registry.register(Command::with_spec(
        "job.describe",
        "Describe a job",
        CommandScope::Workspace,
        CommandSpec::job_describe(),
    ));
    registry.register(Command::with_spec(
        "job.focus-output",
        "Focus the output buffer for a job",
        CommandScope::Workspace,
        CommandSpec::job_focus_output(),
    ));
    registry.register(Command::with_spec(
        "job.list",
        "List jobs",
        CommandScope::Workspace,
        CommandSpec::job_list(),
    ));
    registry.register(Command::with_spec(
        "job.next",
        "Select the next job row",
        CommandScope::Workspace,
        CommandSpec::job_next(),
    ));
    registry.register(Command::with_spec(
        "job.prev",
        "Select the previous job row",
        CommandScope::Workspace,
        CommandSpec::job_previous(),
    ));
    registry.register(Command::with_spec(
        "job.open",
        "Open a jobs buffer",
        CommandScope::Workspace,
        CommandSpec::job_open(),
    ));
    registry.register(Command::with_spec(
        "buffer.structured.current",
        "Describe the selected structured item",
        CommandScope::Workspace,
        CommandSpec::buffer_structured_current(),
    ));
    registry.register(Command::with_spec(
        "buffer.structured.open",
        "Open the linked target for the selected structured item",
        CommandScope::Workspace,
        CommandSpec::buffer_structured_open(),
    ));
    registry.register(Command::with_spec(
        "buffer.structured.next",
        "Select the next structured item",
        CommandScope::Workspace,
        CommandSpec::buffer_structured_next(),
    ));
    registry.register(Command::with_spec(
        "buffer.structured.prev",
        "Select the previous structured item",
        CommandScope::Workspace,
        CommandSpec::buffer_structured_previous(),
    ));
    registry.register(Command::with_spec(
        "buffer.record.current",
        "Describe the selected record row",
        CommandScope::Workspace,
        CommandSpec::buffer_record_current(),
    ));
    registry.register(Command::with_spec(
        "buffer.record.open",
        "Open the linked buffer for the selected record",
        CommandScope::Workspace,
        CommandSpec::buffer_record_open(),
    ));
    registry.register(Command::with_spec(
        "buffer.record.next",
        "Select the next record row",
        CommandScope::Workspace,
        CommandSpec::buffer_record_next(),
    ));
    registry.register(Command::with_spec(
        "buffer.record.prev",
        "Select the previous record row",
        CommandScope::Workspace,
        CommandSpec::buffer_record_previous(),
    ));
    registry.register(Command::with_spec(
        "buffer.tree.current",
        "Describe the selected tree node",
        CommandScope::Workspace,
        CommandSpec::buffer_tree_current(),
    ));
    registry.register(Command::with_spec(
        "buffer.tree.open",
        "Open the linked buffer for the selected tree node",
        CommandScope::Workspace,
        CommandSpec::buffer_tree_open(),
    ));
    registry.register(Command::with_spec(
        "buffer.tree.next",
        "Select the next tree node",
        CommandScope::Workspace,
        CommandSpec::buffer_tree_next(),
    ));
    registry.register(Command::with_spec(
        "buffer.tree.prev",
        "Select the previous tree node",
        CommandScope::Workspace,
        CommandSpec::buffer_tree_previous(),
    ));
    registry.register(Command::with_spec(
        "terminal.open",
        "Open a terminal buffer",
        CommandScope::Workspace,
        CommandSpec::terminal_open(),
    ));
    registry.register(Command::with_spec(
        "terminal.append",
        "Append transcript text to a terminal buffer",
        CommandScope::Workspace,
        CommandSpec::terminal_append(),
    ));
    registry.register(Command::with_spec(
        "browser.url.set",
        "Set a browser buffer url",
        CommandScope::Workspace,
        CommandSpec::browser_set_url(),
    ));
    registry.register(Command::with_spec(
        "browser.title.set",
        "Set a browser buffer title",
        CommandScope::Workspace,
        CommandSpec::browser_set_title(),
    ));
    registry.register(Command::with_spec(
        "canvas.open",
        "Open a canvas buffer",
        CommandScope::Workspace,
        CommandSpec::canvas_open(),
    ));
    registry.register(Command::with_spec(
        "buffer.new",
        "Create a text buffer",
        CommandScope::Workspace,
        CommandSpec::buffer_new(),
    ));
    registry.register(Command::with_spec(
        "help",
        "Show command help",
        CommandScope::Global,
        CommandSpec::help(),
    ));
    registry.register(Command::with_spec(
        "media.open",
        "Open a media buffer",
        CommandScope::Workspace,
        CommandSpec::media_open(),
    ));
    registry.register(Command::with_spec(
        "media.source.set",
        "Set a media buffer source",
        CommandScope::Workspace,
        CommandSpec::media_set_source(),
    ));
    registry.register(Command::with_spec(
        "pane.split.vertical",
        "Split the active pane vertically",
        CommandScope::Workspace,
        CommandSpec::split_pane_vertical(),
    ));
    registry.register(Command::with_spec(
        "pane.next",
        "Focus the next pane",
        CommandScope::Workspace,
        CommandSpec::focus_next_pane(),
    ));
    registry.register(Command::with_spec(
        "pane.prev",
        "Focus the previous pane",
        CommandScope::Workspace,
        CommandSpec::focus_previous_pane(),
    ));
    registry.register(Command::with_spec(
        "pane.left",
        "Focus the pane to the left",
        CommandScope::Workspace,
        CommandSpec::focus_pane_left(),
    ));
    registry.register(Command::with_spec(
        "pane.right",
        "Focus the pane to the right",
        CommandScope::Workspace,
        CommandSpec::focus_pane_right(),
    ));
    registry.register(Command::with_spec(
        "pane.up",
        "Focus the pane above",
        CommandScope::Workspace,
        CommandSpec::focus_pane_up(),
    ));
    registry.register(Command::with_spec(
        "pane.down",
        "Focus the pane below",
        CommandScope::Workspace,
        CommandSpec::focus_pane_down(),
    ));
    registry.register(Command::with_spec(
        "canvas.name.set",
        "Set a canvas buffer name",
        CommandScope::Workspace,
        CommandSpec::canvas_set_name(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "help");

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "Commands: browser.open, browser.title.set, browser.url.set, buffer.describe, buffer.new, buffer.record.current, buffer.record.next, buffer.record.open, buffer.record.prev, buffer.structured.current, buffer.structured.next, buffer.structured.open, buffer.structured.prev, buffer.tree.current, buffer.tree.next, buffer.tree.open, buffer.tree.prev, canvas.name.set, canvas.open, help, job.cancel, job.describe, job.focus-output, job.list, job.next, job.open, job.prev, media.open, media.source.set, pane.down, pane.left, pane.next, pane.prev, pane.right, pane.split.vertical, pane.up, terminal.append, terminal.open, workspace.load, workspace.save".to_string()
        )]
    );
}

#[test]
fn dispatch_help_for_command_uses_command_metadata() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "help",
        "Show command help",
        CommandScope::Global,
        CommandSpec::help(),
    ));
    registry.register(Command::with_spec(
        "buffer.new",
        "Create a text buffer",
        CommandScope::Workspace,
        CommandSpec::buffer_new(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "help buffer.new");

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "Create a text buffer: Usage: :buffer.new <name>".to_string()
        )]
    );
}

#[test]
fn parse_lowers_buffer_new_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "buffer.new",
        "Create a text buffer",
        CommandScope::Workspace,
        CommandSpec::buffer_new(),
    ));

    let parsed = registry.parse("buffer.new notes");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::BufferNew {
            name: "notes".to_string()
        })
    );
}

#[test]
fn parse_lowers_workspace_save_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "workspace.save",
        "Save workspace to a snapshot file",
        CommandScope::Workspace,
        CommandSpec::workspace_save(),
    ));

    let parsed = registry.parse("workspace.save state.json");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::WorkspaceSave {
            path: "state.json".to_string()
        })
    );
}

#[test]
fn parse_lowers_workspace_load_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "workspace.load",
        "Load workspace from a snapshot file",
        CommandScope::Workspace,
        CommandSpec::workspace_load(),
    ));

    let parsed = registry.parse("workspace.load state.json");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::WorkspaceLoad {
            path: "state.json".to_string()
        })
    );
}

#[test]
fn parse_lowers_window_new_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "window.new",
        "Open a new native window",
        CommandScope::Global,
        CommandSpec::window_new(),
    ));

    let parsed = registry.parse("window.new");

    assert_eq!(parsed, CommandRequest::Invocation(CommandInvocation::NewWindow));
}

#[test]
fn parse_lowers_job_list_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "job.list",
        "List jobs",
        CommandScope::Workspace,
        CommandSpec::job_list(),
    ));

    let parsed = registry.parse("job.list");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::JobList)
    );
}

#[test]
fn parse_lowers_job_describe_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "job.describe",
        "Describe a job",
        CommandScope::Workspace,
        CommandSpec::job_describe(),
    ));

    let parsed = registry.parse("job.describe 2");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::JobDescribe {
            job_id: Some(crate::kernel::JobId::new(2))
        })
    );
}

#[test]
fn parse_lowers_job_open_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "job.open",
        "Open a jobs buffer",
        CommandScope::Workspace,
        CommandSpec::job_open(),
    ));

    let parsed = registry.parse("job.open");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::JobOpen)
    );
}

#[test]
fn parse_lowers_buffer_structured_next_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "buffer.structured.next",
        "Select the next structured item",
        CommandScope::Workspace,
        CommandSpec::buffer_structured_next(),
    ));

    let parsed = registry.parse("buffer.structured.next");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::BufferStructuredNext)
    );
}

#[test]
fn dispatch_buffer_record_next_alias_still_works() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_records_buffer("results");
    assert!(workspace.push_record_to_buffer(buffer_id, serde_json::json!({"id": 1})));
    assert!(workspace.push_record_to_buffer(buffer_id, serde_json::json!({"id": 2})));
    assert!(workspace.focus_buffer(buffer_id));

    let result = registry.dispatch_invocation(&mut workspace, CommandInvocation::BufferRecordNext);

    assert_eq!(workspace.selected_record_row(), Some(1));
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "Selected record row 2".to_string()
        )]
    );
}

#[test]
fn dispatch_buffer_tree_open_alias_still_works() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let target = workspace.create_text_buffer("target");
    let mut root = crate::kernel::TreeNode::new("root", "Root");
    root.push_child(crate::kernel::TreeNode::with_linked_buffer(
        "child", "Child", target,
    ));
    let tree = workspace.create_buffer(
        "tree",
        crate::kernel::BufferContent::Tree(crate::kernel::TreeContent::new(vec![root])),
    );
    assert!(workspace.focus_buffer(tree));
    assert!(workspace.select_tree_node("child"));

    let result = registry.dispatch_invocation(&mut workspace, CommandInvocation::BufferTreeOpen);

    assert_eq!(workspace.current_buffer().id(), target);
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(format!(
            "Focused linked buffer {target}"
        ))]
    );
}

#[test]
fn parse_lowers_buffer_record_next_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "buffer.record.next",
        "Select the next record row",
        CommandScope::Workspace,
        CommandSpec::buffer_record_next(),
    ));

    let parsed = registry.parse("buffer.record.next");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::BufferRecordNext)
    );
}

#[test]
fn parse_lowers_job_focus_output_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "job.focus-output",
        "Focus the output buffer for a job",
        CommandScope::Workspace,
        CommandSpec::job_focus_output(),
    ));

    let parsed = registry.parse("job.focus-output 2");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::JobFocusOutput {
            job_id: Some(crate::kernel::JobId::new(2))
        })
    );
}

#[test]
fn parse_lowers_job_cancel_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "job.cancel",
        "Cancel a job",
        CommandScope::Workspace,
        CommandSpec::job_cancel(),
    ));

    let parsed = registry.parse("job.cancel 2");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::JobCancel {
            job_id: Some(crate::kernel::JobId::new(2))
        })
    );
}

#[test]
fn parse_lowers_terminal_open_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "terminal.open",
        "Open a terminal buffer",
        CommandScope::Workspace,
        CommandSpec::terminal_open(),
    ));

    let parsed = registry.parse("terminal.open shell");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::TerminalOpen {
            name: "shell".to_string()
        })
    );
}

#[test]
fn parse_lowers_terminal_append_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "terminal.append",
        "Append transcript text to a terminal buffer",
        CommandScope::Workspace,
        CommandSpec::terminal_append(),
    ));

    let parsed = registry.parse("terminal.append 2 hello world");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::TerminalAppend {
            buffer_id: Some(BufferId::new(2)),
            text: "hello world".to_string()
        })
    );
}

#[test]
fn parse_lowers_terminal_append_without_buffer_id_to_active_target() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "terminal.append",
        "Append transcript text to a terminal buffer",
        CommandScope::Workspace,
        CommandSpec::terminal_append(),
    ));

    let parsed = registry.parse("terminal.append hello world");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::TerminalAppend {
            buffer_id: None,
            text: "hello world".to_string()
        })
    );
}

#[test]
fn parse_lowers_browser_open_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "browser.open",
        "Open a browser buffer",
        CommandScope::Workspace,
        CommandSpec::browser_open(),
    ));

    let parsed = registry.parse("browser.open https://example.com");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::BrowserOpen {
            url: "https://example.com".to_string()
        })
    );
}

#[test]
fn parse_lowers_browser_url_set_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "browser.url.set",
        "Set a browser buffer url",
        CommandScope::Workspace,
        CommandSpec::browser_set_url(),
    ));

    let parsed = registry.parse("browser.url.set 2 https://after.example");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::BrowserSetUrl {
            buffer_id: Some(BufferId::new(2)),
            url: "https://after.example".to_string()
        })
    );
}

#[test]
fn parse_lowers_browser_url_set_without_buffer_id_to_active_target() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "browser.url.set",
        "Set a browser buffer url",
        CommandScope::Workspace,
        CommandSpec::browser_set_url(),
    ));

    let parsed = registry.parse("browser.url.set https://after.example");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::BrowserSetUrl {
            buffer_id: None,
            url: "https://after.example".to_string()
        })
    );
}

#[test]
fn parse_lowers_browser_title_set_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "browser.title.set",
        "Set a browser buffer title",
        CommandScope::Workspace,
        CommandSpec::browser_set_title(),
    ));

    let parsed = registry.parse("browser.title.set 2 Example Domain");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::BrowserSetTitle {
            buffer_id: Some(BufferId::new(2)),
            title: "Example Domain".to_string()
        })
    );
}

#[test]
fn parse_lowers_browser_title_set_without_buffer_id_to_active_target() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "browser.title.set",
        "Set a browser buffer title",
        CommandScope::Workspace,
        CommandSpec::browser_set_title(),
    ));

    let parsed = registry.parse("browser.title.set Example Domain");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::BrowserSetTitle {
            buffer_id: None,
            title: "Example Domain".to_string()
        })
    );
}

#[test]
fn parse_lowers_media_open_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "media.open",
        "Open a media buffer",
        CommandScope::Workspace,
        CommandSpec::media_open(),
    ));

    let parsed = registry.parse("media.open ./clip.mp4");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::MediaOpen {
            source: "./clip.mp4".to_string()
        })
    );
}

#[test]
fn parse_lowers_media_source_set_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "media.source.set",
        "Set a media buffer source",
        CommandScope::Workspace,
        CommandSpec::media_set_source(),
    ));

    let parsed = registry.parse("media.source.set 2 ./after.mp4");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::MediaSetSource {
            buffer_id: Some(BufferId::new(2)),
            source: "./after.mp4".to_string()
        })
    );
}

#[test]
fn parse_lowers_media_source_set_without_buffer_id_to_active_target() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "media.source.set",
        "Set a media buffer source",
        CommandScope::Workspace,
        CommandSpec::media_set_source(),
    ));

    let parsed = registry.parse("media.source.set ./after.mp4");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::MediaSetSource {
            buffer_id: None,
            source: "./after.mp4".to_string()
        })
    );
}

#[test]
fn parse_lowers_canvas_open_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "canvas.open",
        "Open a canvas buffer",
        CommandScope::Workspace,
        CommandSpec::canvas_open(),
    ));

    let parsed = registry.parse("canvas.open playground");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::CanvasOpen {
            name: "playground".to_string()
        })
    );
}

#[test]
fn parse_lowers_canvas_name_set_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "canvas.name.set",
        "Set a canvas buffer name",
        CommandScope::Workspace,
        CommandSpec::canvas_set_name(),
    ));

    let parsed = registry.parse("canvas.name.set 2 after");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::CanvasSetName {
            buffer_id: Some(BufferId::new(2)),
            name: "after".to_string()
        })
    );
}

#[test]
fn parse_lowers_canvas_name_set_without_buffer_id_to_active_target() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "canvas.name.set",
        "Set a canvas buffer name",
        CommandScope::Workspace,
        CommandSpec::canvas_set_name(),
    ));

    let parsed = registry.parse("canvas.name.set after");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::CanvasSetName {
            buffer_id: None,
            name: "after".to_string()
        })
    );
}

#[test]
fn parse_eval_without_script_returns_usage_status() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "eval",
        "Evaluate Lua code",
        CommandScope::Workspace,
        CommandSpec::eval_lua(),
    ));

    let parsed = registry.parse("eval");

    assert_eq!(
        parsed,
        CommandRequest::Status("Usage: :eval <lua code>".to_string())
    );
}

#[test]
fn parse_package_run_lowers_to_typed_invocation() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "package.run",
        "Run a Zacor package command",
        CommandScope::Workspace,
        CommandSpec::package_run(),
    ));

    let parsed = registry.parse("package.run echo default value=hello");

    assert_eq!(
        parsed,
        CommandRequest::Invocation(CommandInvocation::PackageRun {
            package: "echo".to_string(),
            command: "default".to_string(),
            args: [("value".to_string(), "hello".to_string())]
                .into_iter()
                .collect(),
        })
    );
}

#[test]
fn parse_registered_placeholder_command_returns_registered_status() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::new(
        "inspect",
        "Inspect current buffer",
        CommandScope::Buffer,
    ));

    let parsed = registry.parse("inspect");

    assert_eq!(
        parsed,
        CommandRequest::Status("Command registered but not dispatched yet: :inspect".to_string())
    );
}

#[test]
fn parse_quit_with_extra_args_stays_unknown() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "quit",
        "Quit zred",
        CommandScope::Global,
        CommandSpec::quit(),
    ));

    let parsed = registry.parse("quit now");

    assert_eq!(
        parsed,
        CommandRequest::Status("Unknown command: :quit now".to_string())
    );
}

#[test]
fn dispatch_eval_returns_lua_effect() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "eval",
        "Evaluate Lua code",
        CommandScope::Workspace,
        CommandSpec::eval_lua(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "eval minibuffer.message('hi')");

    assert_eq!(
        result.effects(),
        &[CommandEffect::EvalLua(
            "minibuffer.message('hi')".to_string()
        )]
    );
}

#[test]
fn dispatch_split_pane_creates_new_active_pane() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "pane.split.vertical",
        "Split the active pane vertically",
        CommandScope::Workspace,
        CommandSpec::split_pane_vertical(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "pane.split.vertical");

    assert_eq!(workspace.pane_count(), 2);
    assert_eq!(workspace.active_pane_id(), crate::kernel::PaneId::new(2));
    assert_eq!(workspace.current_buffer().name(), "*scratch*");
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "Split pane vertically".to_string()
        )]
    );
}

#[test]
fn dispatch_pane_next_cycles_focus_to_another_pane() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "pane.next",
        "Focus the next pane",
        CommandScope::Workspace,
        CommandSpec::focus_next_pane(),
    ));
    let mut workspace = Workspace::new();
    workspace.split_active_pane(crate::kernel::SplitAxis::Vertical);

    let result = registry.dispatch(&mut workspace, "pane.next");

    assert_eq!(workspace.active_pane_id(), crate::kernel::PaneId::new(1));
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus("Focused pane 1".to_string())]
    );
}

#[test]
fn dispatch_pane_next_reports_when_no_other_pane_exists() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "pane.next",
        "Focus the next pane",
        CommandScope::Workspace,
        CommandSpec::focus_next_pane(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "pane.next");

    assert_eq!(workspace.active_pane_id(), crate::kernel::PaneId::new(1));
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "No other pane to focus".to_string()
        )]
    );
}

#[test]
fn dispatch_pane_right_and_down_follow_split_geometry() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "pane.right",
        "Focus the pane to the right",
        CommandScope::Workspace,
        CommandSpec::focus_pane_right(),
    ));
    registry.register(Command::with_spec(
        "pane.down",
        "Focus the pane below",
        CommandScope::Workspace,
        CommandSpec::focus_pane_down(),
    ));
    let mut workspace = Workspace::new();
    workspace.split_active_pane(crate::kernel::SplitAxis::Vertical);
    workspace.split_active_pane(crate::kernel::SplitAxis::Horizontal);
    workspace.focus_previous_pane();
    workspace.focus_previous_pane();

    let right = registry.dispatch(&mut workspace, "pane.right");
    assert_eq!(workspace.active_pane_id(), crate::kernel::PaneId::new(2));
    assert_eq!(
        right.effects(),
        &[CommandEffect::SetStatus("Focused pane 2".to_string())]
    );

    let down = registry.dispatch(&mut workspace, "pane.down");
    assert_eq!(workspace.active_pane_id(), crate::kernel::PaneId::new(3));
    assert_eq!(
        down.effects(),
        &[CommandEffect::SetStatus("Focused pane 3".to_string())]
    );
}

#[test]
fn dispatch_pane_resize_left_updates_split_ratio() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "pane.resize.left",
        "Grow the active pane to the left",
        CommandScope::Workspace,
        CommandSpec::resize_pane_left(),
    ));
    let mut workspace = Workspace::new();
    workspace.split_active_pane(crate::kernel::SplitAxis::Vertical);

    let result = registry.dispatch(&mut workspace, "pane.resize.left");

    assert!(matches!(
        workspace.pane_tree().root(),
        crate::kernel::PaneNode::Split {
            ratio_percent: 40,
            ..
        }
    ));
    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus("Resized pane left".to_string())]
    );
}

#[test]
fn dispatch_pane_resize_reports_when_no_matching_split_exists() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "pane.resize.left",
        "Grow the active pane to the left",
        CommandScope::Workspace,
        CommandSpec::resize_pane_left(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "pane.resize.left");

    assert_eq!(
        result.effects(),
        &[CommandEffect::SetStatus(
            "No pane to resize left".to_string()
        )]
    );
}

#[test]
fn dispatch_package_run_creates_job_and_records_buffer() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "package.run",
        "Run a Zacor package command",
        CommandScope::Workspace,
        CommandSpec::package_run(),
    ));
    let mut workspace = Workspace::new();

    let result = registry.dispatch(&mut workspace, "package.run echo default value=hi");

    let Some(CommandData::PackageJobStarted { job_id, buffer_id }) = result.data() else {
        panic!("expected package job data, got {:?}", result.data());
    };
    let buffer = workspace
        .buffer(*buffer_id)
        .expect("records buffer should exist");
    let job = workspace.jobs().get(*job_id).expect("job should exist");

    assert!(matches!(buffer.content(), BufferContent::Records(_)));
    assert_eq!(buffer.name(), "*pkg:echo default*");
    assert_eq!(workspace.current_buffer().id(), *buffer_id);
    assert_eq!(job.name(), "package echo default");
    assert_eq!(
        job.kind(),
        &JobKind::PackageInvoke {
            package: "echo".to_string(),
            command: "default".to_string(),
            output_buffer_id: *buffer_id,
        }
    );
    assert_eq!(
        result.effects(),
        &[
            CommandEffect::SetStatus("Running echo default".to_string()),
            CommandEffect::InvokePackage(PackageInvocationRequest {
                job_id: *job_id,
                buffer_id: *buffer_id,
                package: "echo".to_string(),
                command: "default".to_string(),
                args: [("value".to_string(), "hi".to_string())]
                    .into_iter()
                    .collect(),
            }),
        ]
    );
}

#[test]
fn typed_buffer_append_reports_missing_buffer_as_error() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::BufferAppend {
            buffer_id: BufferId::new(999),
            text: "hello".to_string(),
        },
    );

    assert_eq!(result.error(), Some("unknown buffer id: 999"));
}

#[test]
fn typed_browser_set_title_reports_non_browser_buffer_as_error() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_text_buffer("notes");

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::BrowserSetTitle {
            buffer_id: Some(buffer_id),
            title: "Example".to_string(),
        },
    );

    assert_eq!(
        result.error(),
        Some("buffer does not accept browser title: 2")
    );
}

#[test]
fn typed_browser_set_url_reports_non_browser_buffer_as_error() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_text_buffer("notes");

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::BrowserSetUrl {
            buffer_id: Some(buffer_id),
            url: "https://example.com".to_string(),
        },
    );

    assert_eq!(
        result.error(),
        Some("buffer does not accept browser url: 2")
    );
}

#[test]
fn typed_media_set_source_reports_non_media_buffer_as_error() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_text_buffer("notes");

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::MediaSetSource {
            buffer_id: Some(buffer_id),
            source: "./clip.mp4".to_string(),
        },
    );

    assert_eq!(
        result.error(),
        Some("buffer does not accept media source: 2")
    );
}

#[test]
fn typed_terminal_append_reports_non_terminal_buffer_as_error() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_text_buffer("notes");

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::TerminalAppend {
            buffer_id: Some(buffer_id),
            text: "hello".to_string(),
        },
    );

    assert_eq!(
        result.error(),
        Some("buffer does not accept terminal append: 2")
    );
}

#[test]
fn typed_canvas_set_name_reports_non_canvas_buffer_as_error() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_text_buffer("notes");

    let result = registry.dispatch_invocation(
        &mut workspace,
        CommandInvocation::CanvasSetName {
            buffer_id: Some(buffer_id),
            name: "playground".to_string(),
        },
    );

    assert_eq!(
        result.error(),
        Some("buffer does not accept canvas name: 2")
    );
}

#[test]
fn command_usage_is_exposed_by_spec() {
    let command = Command::with_spec(
        "eval",
        "Evaluate Lua code",
        CommandScope::Workspace,
        CommandSpec::eval_lua(),
    );

    assert_eq!(command.usage(), Some("Usage: :eval <lua code>"));
}

#[test]
fn registry_entries_expose_command_metadata() {
    let mut registry = CommandRegistry::new();
    registry.register(Command::with_spec(
        "eval",
        "Evaluate Lua code",
        CommandScope::Workspace,
        CommandSpec::eval_lua(),
    ));

    let entries = registry.entries().collect::<Vec<_>>();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name(), "eval");
    assert_eq!(entries[0].summary(), "Evaluate Lua code");
    assert_eq!(entries[0].scope(), CommandScope::Workspace);
    assert_eq!(entries[0].usage(), Some("Usage: :eval <lua code>"));
}

#[test]
fn typed_buffer_focus_updates_active_buffer_without_effects() {
    let registry = CommandRegistry::new();
    let mut workspace = Workspace::new();
    let buffer_id = workspace.create_text_buffer("notes");

    let result =
        registry.dispatch_invocation(&mut workspace, CommandInvocation::BufferFocus { buffer_id });

    assert!(result.effects().is_empty());
    assert_eq!(result.data(), None);
    assert_eq!(result.error(), None);
    assert_eq!(workspace.current_buffer().name(), "notes");
}
