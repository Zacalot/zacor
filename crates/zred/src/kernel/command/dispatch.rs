use super::registry::CommandRegistry;
use super::types::{
    CommandData, CommandEffect, CommandInvocation, CommandRequest, CommandResult,
    PackageInvocationRequest,
};
use crate::kernel::buffer::BufferContent;
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
