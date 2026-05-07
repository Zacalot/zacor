use super::registry::CommandRegistry;
use super::types::{
    CommandData, CommandEffect, CommandInvocation, CommandRequest, CommandResult,
    PackageInvocationRequest,
};
use crate::kernel::buffer::{
    BrowserContent, BufferContent, CanvasContent, MediaContent, RecordsContent, TerminalContent,
};
use crate::kernel::workspace::Workspace;

impl CommandRegistry {
    pub fn dispatch(&self, workspace: &mut Workspace, input: &str) -> CommandResult {
        let request = self.parse(input);
        self.dispatch_request(workspace, request)
    }

    pub fn dispatch_request(
        &self,
        workspace: &mut Workspace,
        request: CommandRequest,
    ) -> CommandResult {
        match request {
            CommandRequest::Invocation(invocation) => {
                self.dispatch_invocation(workspace, invocation)
            }
            CommandRequest::Status(status) => {
                CommandResult::with_effect(CommandEffect::SetStatus(status))
            }
        }
    }

    pub fn dispatch_invocation(
        &self,
        workspace: &mut Workspace,
        invocation: CommandInvocation,
    ) -> CommandResult {
        match invocation {
            CommandInvocation::Quit => CommandResult::with_effect(CommandEffect::Quit),
            CommandInvocation::NewWindow => {
                let mut result = CommandResult::with_effect(CommandEffect::NewWindow);
                result.push(CommandEffect::SetStatus("Opened a new window".to_string()));
                result
            }
            CommandInvocation::Help { name } => {
                let status = match name {
                    Some(name) => match self.get(&name) {
                        Some(command) => match command.usage() {
                            Some(usage) => format!("{}: {}", command.summary(), usage),
                            None => format!("{}: {}", command.name(), command.summary()),
                        },
                        None => format!("Unknown command: :{name}"),
                    },
                    None => {
                        let names = self
                            .entries()
                            .map(|entry| entry.name().to_string())
                            .collect::<Vec<_>>();
                        format!("Commands: {}", names.join(", "))
                    }
                };
                CommandResult::with_effect(CommandEffect::SetStatus(status))
            }
            CommandInvocation::EvalLua { script } => {
                CommandResult::with_effect(CommandEffect::EvalLua(script))
            }
            CommandInvocation::WorkspaceSave { path } => {
                let mut result =
                    CommandResult::with_effect(CommandEffect::SaveWorkspace(path.clone()));
                result.push(CommandEffect::SetStatus(format!(
                    "Saved workspace to {path}"
                )));
                result
            }
            CommandInvocation::WorkspaceLoad { path } => {
                let mut result =
                    CommandResult::with_effect(CommandEffect::LoadWorkspace(path.clone()));
                result.push(CommandEffect::SetStatus(format!(
                    "Loaded workspace from {path}"
                )));
                result
            }
            CommandInvocation::JobList => {
                CommandResult::with_effect(CommandEffect::SetStatus(describe_jobs(workspace)))
            }
            CommandInvocation::JobNext => move_job_selection(workspace, 1),
            CommandInvocation::JobPrevious => move_job_selection(workspace, -1),
            CommandInvocation::JobOpen => open_or_refresh_jobs_buffer(workspace),
            CommandInvocation::BufferStructuredCurrent => {
                describe_selected_structured_item(workspace)
            }
            CommandInvocation::BufferStructuredOpen => focus_selected_structured_item(workspace),
            CommandInvocation::BufferStructuredNext => move_structured_selection(workspace, 1),
            CommandInvocation::BufferStructuredPrevious => move_structured_selection(workspace, -1),
            CommandInvocation::BufferRecordCurrent => describe_selected_record(workspace),
            CommandInvocation::BufferRecordOpen => focus_selected_record_buffer(workspace),
            CommandInvocation::BufferRecordNext => move_record_selection(workspace, 1),
            CommandInvocation::BufferRecordPrevious => move_record_selection(workspace, -1),
            CommandInvocation::BufferTreeCurrent => describe_selected_tree_node(workspace),
            CommandInvocation::BufferTreeOpen => focus_selected_tree_buffer(workspace),
            CommandInvocation::BufferTreeNext => move_tree_selection(workspace, 1),
            CommandInvocation::BufferTreePrevious => move_tree_selection(workspace, -1),
            CommandInvocation::JobDescribe { job_id } => match resolve_job_id(workspace, job_id) {
                Some(job_id) => match workspace.jobs().get(job_id) {
                    Some(job) => {
                        CommandResult::with_effect(CommandEffect::SetStatus(describe_job(job)))
                    }
                    None => CommandResult::with_error(format!("unknown job id: {job_id}")),
                },
                None => CommandResult::with_error("no selected job"),
            },
            CommandInvocation::JobFocusOutput { job_id } => {
                focus_job_output_buffer(workspace, resolve_job_id(workspace, job_id))
            }
            CommandInvocation::JobCancel { job_id } => match resolve_job_id(workspace, job_id) {
                Some(job_id) => {
                    if workspace.cancel_job(job_id) {
                        CommandResult::with_effect(CommandEffect::SetStatus(format!(
                            "Cancelled job {job_id}"
                        )))
                    } else {
                        CommandResult::with_error(format!("unknown job id: {job_id}"))
                    }
                }
                None => CommandResult::with_error("no selected job"),
            },
            CommandInvocation::BufferDescribe => {
                let buffer = workspace.current_buffer();
                CommandResult::with_effect(CommandEffect::SetStatus(describe_buffer(buffer)))
            }
            CommandInvocation::TerminalOpen { name } => open_created_buffer(
                workspace,
                format!("*terminal:{name}*"),
                BufferContent::Terminal(TerminalContent::default()),
                format!("Opened terminal {name}"),
            ),
            CommandInvocation::TerminalAppend { buffer_id, text } => {
                let buffer_id = buffer_id.unwrap_or_else(|| workspace.current_buffer().id());
                update_existing_buffer(
                    workspace,
                    buffer_id,
                    |workspace| workspace.append_to_terminal_buffer(buffer_id, &text),
                    format!("Appended terminal output for {buffer_id}"),
                    format!("buffer does not accept terminal append: {buffer_id}"),
                )
            }
            CommandInvocation::BufferNew { name } => {
                let buffer_id =
                    workspace.create_buffer(&name, BufferContent::Text(Default::default()));
                workspace.append_to_buffer(buffer_id, &format!("Buffer {name}"));
                workspace.focus_buffer(buffer_id);
                CommandResult::with_data_and_effect(
                    CommandData::BufferCreated { buffer_id },
                    CommandEffect::SetStatus(format!("Created {name}")),
                )
            }
            CommandInvocation::BrowserOpen { url } => open_created_buffer(
                workspace,
                format!("*browser:{url}*"),
                BufferContent::Browser(BrowserContent::new(Some(url.clone()), None)),
                format!("Opened {url}"),
            ),
            CommandInvocation::BrowserSetUrl { buffer_id, url } => {
                let buffer_id = buffer_id.unwrap_or_else(|| workspace.current_buffer().id());
                update_existing_buffer(
                    workspace,
                    buffer_id,
                    |workspace| workspace.set_browser_url(buffer_id, &url),
                    format!("Set browser url for {buffer_id}"),
                    format!("buffer does not accept browser url: {buffer_id}"),
                )
            }
            CommandInvocation::BrowserSetTitle { buffer_id, title } => {
                let buffer_id = buffer_id.unwrap_or_else(|| workspace.current_buffer().id());
                update_existing_buffer(
                    workspace,
                    buffer_id,
                    |workspace| workspace.set_browser_title(buffer_id, &title),
                    format!("Set browser title for {buffer_id}"),
                    format!("buffer does not accept browser title: {buffer_id}"),
                )
            }
            CommandInvocation::MediaOpen { source } => open_created_buffer(
                workspace,
                format!("*media:{source}*"),
                BufferContent::Media(MediaContent::new(Some(source.clone()))),
                format!("Opened {source}"),
            ),
            CommandInvocation::MediaSetSource { buffer_id, source } => {
                let buffer_id = buffer_id.unwrap_or_else(|| workspace.current_buffer().id());
                update_existing_buffer(
                    workspace,
                    buffer_id,
                    |workspace| workspace.set_media_source(buffer_id, &source),
                    format!("Set media source for {buffer_id}"),
                    format!("buffer does not accept media source: {buffer_id}"),
                )
            }
            CommandInvocation::CanvasOpen { name } => open_created_buffer(
                workspace,
                format!("*canvas:{name}*"),
                BufferContent::Canvas(CanvasContent::new(Some(name.clone()))),
                format!("Opened {name}"),
            ),
            CommandInvocation::CanvasSetName { buffer_id, name } => {
                let buffer_id = buffer_id.unwrap_or_else(|| workspace.current_buffer().id());
                update_existing_buffer(
                    workspace,
                    buffer_id,
                    |workspace| workspace.set_canvas_name(buffer_id, &name),
                    format!("Set canvas name for {buffer_id}"),
                    format!("buffer does not accept canvas name: {buffer_id}"),
                )
            }
            CommandInvocation::SplitPane { axis } => {
                workspace.split_active_pane(axis);
                let status = match axis {
                    crate::kernel::SplitAxis::Horizontal => "Split pane horizontally",
                    crate::kernel::SplitAxis::Vertical => "Split pane vertically",
                };
                CommandResult::with_effect(CommandEffect::SetStatus(status.to_string()))
            }
            CommandInvocation::FocusNextPane => match workspace.focus_next_pane() {
                Some(pane_id) => CommandResult::with_effect(CommandEffect::SetStatus(format!(
                    "Focused pane {pane_id}"
                ))),
                None => CommandResult::with_effect(CommandEffect::SetStatus(
                    "No other pane to focus".to_string(),
                )),
            },
            CommandInvocation::FocusPreviousPane => match workspace.focus_previous_pane() {
                Some(pane_id) => CommandResult::with_effect(CommandEffect::SetStatus(format!(
                    "Focused pane {pane_id}"
                ))),
                None => CommandResult::with_effect(CommandEffect::SetStatus(
                    "No other pane to focus".to_string(),
                )),
            },
            CommandInvocation::FocusPaneDirection { direction } => {
                match workspace.focus_pane_direction(direction) {
                    Some(pane_id) => CommandResult::with_effect(CommandEffect::SetStatus(format!(
                        "Focused pane {pane_id}"
                    ))),
                    None => CommandResult::with_effect(CommandEffect::SetStatus(format!(
                        "No pane to focus {}",
                        match direction {
                            crate::kernel::PaneDirection::Left => "left",
                            crate::kernel::PaneDirection::Right => "right",
                            crate::kernel::PaneDirection::Up => "up",
                            crate::kernel::PaneDirection::Down => "down",
                        }
                    ))),
                }
            }
            CommandInvocation::ResizePaneDirection { direction } => {
                if workspace.resize_active_pane(direction, 10) {
                    CommandResult::with_effect(CommandEffect::SetStatus(format!(
                        "Resized pane {}",
                        match direction {
                            crate::kernel::PaneDirection::Left => "left",
                            crate::kernel::PaneDirection::Right => "right",
                            crate::kernel::PaneDirection::Up => "up",
                            crate::kernel::PaneDirection::Down => "down",
                        }
                    )))
                } else {
                    CommandResult::with_effect(CommandEffect::SetStatus(format!(
                        "No pane to resize {}",
                        match direction {
                            crate::kernel::PaneDirection::Left => "left",
                            crate::kernel::PaneDirection::Right => "right",
                            crate::kernel::PaneDirection::Up => "up",
                            crate::kernel::PaneDirection::Down => "down",
                        }
                    )))
                }
            }
            CommandInvocation::PackageRun {
                package,
                command,
                args,
            } => {
                let buffer_name = format!("*pkg:{} {}*", package, command);
                let buffer_id = workspace.create_records_buffer(&buffer_name);
                workspace.focus_buffer(buffer_id);
                let job_id = workspace.create_job_with_kind(
                    &format!("package {} {}", package, command),
                    Some(crate::kernel::JobOwner::Workspace(workspace.id())),
                    crate::kernel::JobKind::PackageInvoke {
                        package: package.clone(),
                        command: command.clone(),
                        output_buffer_id: buffer_id,
                    },
                );

                let mut result = CommandResult::with_data_and_effect(
                    CommandData::PackageJobStarted { job_id, buffer_id },
                    CommandEffect::SetStatus(format!("Running {} {}", package, command)),
                );
                result.push(CommandEffect::InvokePackage(PackageInvocationRequest {
                    job_id,
                    buffer_id,
                    package,
                    command,
                    args,
                }));
                result
            }
            CommandInvocation::BufferAppend { buffer_id, text } => {
                if workspace.append_to_buffer(buffer_id, &text) {
                    CommandResult::new()
                } else if workspace.buffer(buffer_id).is_none() {
                    CommandResult::with_error(format!("unknown buffer id: {buffer_id}"))
                } else {
                    CommandResult::with_error(format!(
                        "buffer does not accept text append: {buffer_id}"
                    ))
                }
            }
            CommandInvocation::BufferSetContents { buffer_id, text } => {
                if workspace.set_buffer_contents(buffer_id, &text) {
                    CommandResult::new()
                } else if workspace.buffer(buffer_id).is_none() {
                    CommandResult::with_error(format!("unknown buffer id: {buffer_id}"))
                } else {
                    CommandResult::with_error(format!(
                        "buffer does not accept text contents: {buffer_id}"
                    ))
                }
            }
            CommandInvocation::BufferFocus { buffer_id } => {
                if workspace.focus_buffer(buffer_id) {
                    CommandResult::new()
                } else {
                    CommandResult::with_error(format!("unknown buffer id: {buffer_id}"))
                }
            }
        }
    }
}

fn open_created_buffer(
    workspace: &mut Workspace,
    buffer_name: String,
    content: BufferContent,
    status: String,
) -> CommandResult {
    let buffer_id = workspace.create_buffer(&buffer_name, content);
    workspace.focus_buffer(buffer_id);
    CommandResult::with_data_and_effect(
        CommandData::BufferCreated { buffer_id },
        CommandEffect::SetStatus(status),
    )
}

fn open_or_refresh_jobs_buffer(workspace: &mut Workspace) -> CommandResult {
    let records = workspace.job_records();
    let count = records.len();

    if let Some(buffer_id) = workspace.find_buffer_by_name("*jobs*") {
        workspace.set_records_buffer_contents(buffer_id, records);
        workspace.focus_buffer(buffer_id);
        return CommandResult::with_effect(CommandEffect::SetStatus(format!(
            "Refreshed jobs buffer ({count})"
        )));
    }

    open_created_buffer(
        workspace,
        "*jobs*".to_string(),
        BufferContent::Records(RecordsContent::new(records)),
        format!("Opened jobs buffer ({count})"),
    )
}

fn move_job_selection(workspace: &mut Workspace, delta: isize) -> CommandResult {
    match workspace.move_selected_job_row(delta) {
        Some(row) => CommandResult::with_effect(CommandEffect::SetStatus(format!(
            "Selected job row {}",
            row + 1
        ))),
        None => CommandResult::with_error("current buffer is not a selectable jobs buffer"),
    }
}

fn move_structured_selection(workspace: &mut Workspace, delta: isize) -> CommandResult {
    match workspace.current_buffer().content() {
        BufferContent::Records(_) => move_record_selection(workspace, delta),
        BufferContent::Tree(_) => move_tree_selection(workspace, delta),
        _ => CommandResult::with_error("current buffer is not a structured buffer"),
    }
}

fn move_record_selection(workspace: &mut Workspace, delta: isize) -> CommandResult {
    match workspace.move_selected_record_row(delta) {
        Some(row) => CommandResult::with_effect(CommandEffect::SetStatus(format!(
            "Selected record row {}",
            row + 1
        ))),
        None => CommandResult::with_error("current buffer is not a selectable records buffer"),
    }
}

fn describe_selected_record(workspace: &Workspace) -> CommandResult {
    match workspace.current_record() {
        Some(record) => {
            CommandResult::with_effect(CommandEffect::SetStatus(format!("Record: {}", record)))
        }
        None => CommandResult::with_error("no selected record"),
    }
}

fn describe_selected_structured_item(workspace: &Workspace) -> CommandResult {
    match workspace.current_buffer().content() {
        BufferContent::Records(_) => describe_selected_record(workspace),
        BufferContent::Tree(_) => describe_selected_tree_node(workspace),
        _ => CommandResult::with_error("current buffer is not a structured buffer"),
    }
}

fn focus_selected_record_buffer(workspace: &mut Workspace) -> CommandResult {
    let Some(buffer_id) = workspace.linked_buffer_id_from_current_record() else {
        return CommandResult::with_error("selected record does not link to a buffer");
    };

    if workspace.focus_buffer(buffer_id) {
        CommandResult::with_effect(CommandEffect::SetStatus(format!(
            "Focused linked buffer {buffer_id}"
        )))
    } else {
        CommandResult::with_error(format!("unknown buffer id: {buffer_id}"))
    }
}

fn focus_selected_structured_item(workspace: &mut Workspace) -> CommandResult {
    match workspace.current_buffer().content() {
        BufferContent::Records(_) => focus_selected_record_buffer(workspace),
        BufferContent::Tree(_) => focus_selected_tree_buffer(workspace),
        _ => CommandResult::with_error("current buffer is not a structured buffer"),
    }
}

fn move_tree_selection(workspace: &mut Workspace, delta: isize) -> CommandResult {
    match workspace.move_selected_tree_node(delta) {
        Some(node_id) => CommandResult::with_effect(CommandEffect::SetStatus(format!(
            "Selected tree node {node_id}"
        ))),
        None => CommandResult::with_error("current buffer is not a selectable tree buffer"),
    }
}

fn describe_selected_tree_node(workspace: &Workspace) -> CommandResult {
    match workspace.current_tree_node() {
        Some(node) => CommandResult::with_effect(CommandEffect::SetStatus(format!(
            "Tree node {}: {}",
            node.id(),
            node.label()
        ))),
        None => CommandResult::with_error("no selected tree node"),
    }
}

fn focus_selected_tree_buffer(workspace: &mut Workspace) -> CommandResult {
    let Some(buffer_id) = workspace.linked_buffer_id_from_current_tree_node() else {
        return CommandResult::with_error("selected tree node does not link to a buffer");
    };

    if workspace.focus_buffer(buffer_id) {
        CommandResult::with_effect(CommandEffect::SetStatus(format!(
            "Focused linked buffer {buffer_id}"
        )))
    } else {
        CommandResult::with_error(format!("unknown buffer id: {buffer_id}"))
    }
}

fn resolve_job_id(
    workspace: &Workspace,
    job_id: Option<crate::kernel::JobId>,
) -> Option<crate::kernel::JobId> {
    job_id.or_else(|| workspace.selected_job_id_from_jobs_buffer())
}

fn focus_job_output_buffer(
    workspace: &mut Workspace,
    job_id: Option<crate::kernel::JobId>,
) -> CommandResult {
    let Some(job_id) = resolve_job_id(workspace, job_id) else {
        return CommandResult::with_error("no selected job");
    };

    let Some(job) = workspace.jobs().get(job_id) else {
        return CommandResult::with_error(format!("unknown job id: {job_id}"));
    };

    let (package, command, output_buffer_id) = match job.kind() {
        crate::kernel::JobKind::PackageInvoke {
            package,
            command,
            output_buffer_id,
        } => (package.clone(), command.clone(), *output_buffer_id),
        crate::kernel::JobKind::Generic => {
            return CommandResult::with_error(format!(
                "job does not have an output buffer: {job_id}"
            ));
        }
    };

    if workspace.focus_buffer(output_buffer_id) {
        CommandResult::with_effect(CommandEffect::SetStatus(format!(
            "Focused output buffer {} for job {} ({} {})",
            output_buffer_id, job_id, package, command
        )))
    } else {
        CommandResult::with_error(format!("unknown buffer id: {output_buffer_id}"))
    }
}

fn update_existing_buffer(
    workspace: &mut Workspace,
    buffer_id: crate::kernel::BufferId,
    update: impl FnOnce(&mut Workspace) -> bool,
    success_status: String,
    wrong_kind_error: String,
) -> CommandResult {
    if update(workspace) {
        CommandResult::with_effect(CommandEffect::SetStatus(success_status))
    } else if workspace.buffer(buffer_id).is_none() {
        CommandResult::with_error(format!("unknown buffer id: {buffer_id}"))
    } else {
        CommandResult::with_error(wrong_kind_error)
    }
}

fn describe_buffer(buffer: &crate::kernel::Buffer) -> String {
    match buffer.content() {
        BufferContent::Text(content) => format!(
            "Buffer {}: text ({} lines)",
            buffer.id(),
            content.lines().len()
        ),
        BufferContent::Records(content) => format!(
            "Buffer {}: records ({} records)",
            buffer.id(),
            content.records().len()
        ),
        BufferContent::Tree(content) => format!(
            "Buffer {}: tree ({} roots)",
            buffer.id(),
            content.roots().len()
        ),
        BufferContent::Terminal(content) => format!(
            "Buffer {}: terminal ({} lines)",
            buffer.id(),
            content.transcript().lines().len()
        ),
        BufferContent::Browser(content) => match (content.url(), content.title()) {
            (Some(url), Some(title)) => {
                format!("Buffer {}: browser {} ({})", buffer.id(), url, title)
            }
            (Some(url), None) => format!("Buffer {}: browser {}", buffer.id(), url),
            (None, Some(title)) => format!("Buffer {}: browser ({})", buffer.id(), title),
            (None, None) => format!("Buffer {}: browser", buffer.id()),
        },
        BufferContent::Media(content) => match content.source() {
            Some(source) => format!("Buffer {}: media {}", buffer.id(), source),
            None => format!("Buffer {}: media", buffer.id()),
        },
        BufferContent::Canvas(content) => match content.name() {
            Some(name) => format!("Buffer {}: canvas {}", buffer.id(), name),
            None => format!("Buffer {}: canvas", buffer.id()),
        },
    }
}

fn describe_jobs(workspace: &Workspace) -> String {
    let jobs = workspace
        .jobs()
        .entries()
        .map(|job| {
            format!(
                "{}:{} [{}]",
                job.id(),
                job.name(),
                describe_job_status(job.status())
            )
        })
        .collect::<Vec<_>>();

    if jobs.is_empty() {
        "No jobs".to_string()
    } else {
        format!("Jobs: {}", jobs.join(", "))
    }
}

fn describe_job(job: &crate::kernel::Job) -> String {
    let kind_suffix = match job.kind() {
        crate::kernel::JobKind::Generic => String::new(),
        crate::kernel::JobKind::PackageInvoke {
            package,
            command,
            output_buffer_id,
        } => format!(" {} {} -> buffer {}", package, command, output_buffer_id),
    };

    format!(
        "Job {}: {} [{}]{}",
        job.id(),
        job.name(),
        describe_job_status(job.status()),
        kind_suffix
    )
}

fn describe_job_status(status: &crate::kernel::JobStatus) -> String {
    match status {
        crate::kernel::JobStatus::Pending => "pending".to_string(),
        crate::kernel::JobStatus::Running => "running".to_string(),
        crate::kernel::JobStatus::Succeeded => "succeeded".to_string(),
        crate::kernel::JobStatus::Failed(message) => format!("failed: {message}"),
        crate::kernel::JobStatus::Cancelled => "cancelled".to_string(),
    }
}
