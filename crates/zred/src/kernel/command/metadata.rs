use super::types::{CommandInvocation, CommandRequest};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Command {
    name: String,
    summary: String,
    scope: CommandScope,
    spec: CommandSpec,
}

impl Command {
    pub fn new(name: impl Into<String>, summary: impl Into<String>, scope: CommandScope) -> Self {
        Self::with_spec(name, summary, scope, CommandSpec::placeholder())
    }

    pub fn with_spec(
        name: impl Into<String>,
        summary: impl Into<String>,
        scope: CommandScope,
        spec: CommandSpec,
    ) -> Self {
        Self {
            name: name.into(),
            summary: summary.into(),
            scope,
            spec,
        }
    }

    pub(crate) fn parse(&self, input: &str, args: Option<&str>) -> CommandRequest {
        match self.spec {
            CommandSpec::Placeholder => CommandRequest::Status(format!(
                "Command registered but not dispatched yet: :{}",
                self.name
            )),
            CommandSpec::Quit => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::Quit)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::WindowNew => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::NewWindow)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::Help { usage: _ } => match args {
                Some(name) if !name.trim().is_empty() => {
                    CommandRequest::Invocation(CommandInvocation::Help {
                        name: Some(name.trim().to_string()),
                    })
                }
                Some(_) | None => {
                    CommandRequest::Invocation(CommandInvocation::Help { name: None })
                }
            },
            CommandSpec::EvalLua { usage } => match args {
                Some(script) => CommandRequest::Invocation(CommandInvocation::EvalLua {
                    script: script.to_string(),
                }),
                None => CommandRequest::Status(usage.to_string()),
            },
            CommandSpec::WorkspaceSave { usage } => match args {
                Some(path) if !path.trim().is_empty() => {
                    CommandRequest::Invocation(CommandInvocation::WorkspaceSave {
                        path: path.trim().to_string(),
                    })
                }
                _ => CommandRequest::Status(usage.to_string()),
            },
            CommandSpec::WorkspaceLoad { usage } => match args {
                Some(path) if !path.trim().is_empty() => {
                    CommandRequest::Invocation(CommandInvocation::WorkspaceLoad {
                        path: path.trim().to_string(),
                    })
                }
                _ => CommandRequest::Status(usage.to_string()),
            },
            CommandSpec::JobList => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::JobList)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::JobNext => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::JobNext)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::JobPrevious => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::JobPrevious)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::JobOpen => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::JobOpen)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::BufferStructuredCurrent => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::BufferStructuredCurrent)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::BufferStructuredOpen => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::BufferStructuredOpen)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::BufferStructuredNext => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::BufferStructuredNext)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::BufferStructuredPrevious => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::BufferStructuredPrevious)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::BufferRecordCurrent => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::BufferRecordCurrent)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::BufferRecordOpen => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::BufferRecordOpen)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::BufferRecordNext => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::BufferRecordNext)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::BufferRecordPrevious => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::BufferRecordPrevious)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::BufferTreeCurrent => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::BufferTreeCurrent)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::BufferTreeOpen => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::BufferTreeOpen)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::BufferTreeNext => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::BufferTreeNext)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::BufferTreePrevious => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::BufferTreePrevious)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::JobFocusOutput { usage } => match parse_optional_job_id(args) {
                Some(job_id) => {
                    CommandRequest::Invocation(CommandInvocation::JobFocusOutput { job_id })
                }
                None => CommandRequest::Status(usage.to_string()),
            },
            CommandSpec::JobDescribe { usage } => match parse_optional_job_id(args) {
                Some(job_id) => {
                    CommandRequest::Invocation(CommandInvocation::JobDescribe { job_id })
                }
                None => CommandRequest::Status(usage.to_string()),
            },
            CommandSpec::JobCancel { usage } => match parse_optional_job_id(args) {
                Some(job_id) => CommandRequest::Invocation(CommandInvocation::JobCancel { job_id }),
                None => CommandRequest::Status(usage.to_string()),
            },
            CommandSpec::BufferDescribe => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::BufferDescribe)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::TerminalOpen { usage } => match args {
                Some(name) if !name.trim().is_empty() => {
                    CommandRequest::Invocation(CommandInvocation::TerminalOpen {
                        name: name.trim().to_string(),
                    })
                }
                _ => CommandRequest::Status(usage.to_string()),
            },
            CommandSpec::TerminalAppend { usage } => match parse_terminal_append_args(args) {
                Some(invocation) => CommandRequest::Invocation(invocation),
                None => CommandRequest::Status(usage.to_string()),
            },
            CommandSpec::BufferNew { usage } => match args {
                Some(name) if !name.trim().is_empty() => {
                    CommandRequest::Invocation(CommandInvocation::BufferNew {
                        name: name.trim().to_string(),
                    })
                }
                _ => CommandRequest::Status(usage.to_string()),
            },
            CommandSpec::BrowserOpen { usage } => match args {
                Some(url) if !url.trim().is_empty() => {
                    CommandRequest::Invocation(CommandInvocation::BrowserOpen {
                        url: url.trim().to_string(),
                    })
                }
                _ => CommandRequest::Status(usage.to_string()),
            },
            CommandSpec::BrowserSetUrl { usage } => match parse_browser_url_set_args(args) {
                Some(invocation) => CommandRequest::Invocation(invocation),
                None => CommandRequest::Status(usage.to_string()),
            },
            CommandSpec::BrowserSetTitle { usage } => match parse_browser_title_set_args(args) {
                Some(invocation) => CommandRequest::Invocation(invocation),
                None => CommandRequest::Status(usage.to_string()),
            },
            CommandSpec::MediaOpen { usage } => match args {
                Some(source) if !source.trim().is_empty() => {
                    CommandRequest::Invocation(CommandInvocation::MediaOpen {
                        source: source.trim().to_string(),
                    })
                }
                _ => CommandRequest::Status(usage.to_string()),
            },
            CommandSpec::MediaSetSource { usage } => match parse_media_source_set_args(args) {
                Some(invocation) => CommandRequest::Invocation(invocation),
                None => CommandRequest::Status(usage.to_string()),
            },
            CommandSpec::CanvasOpen { usage } => match args {
                Some(name) if !name.trim().is_empty() => {
                    CommandRequest::Invocation(CommandInvocation::CanvasOpen {
                        name: name.trim().to_string(),
                    })
                }
                _ => CommandRequest::Status(usage.to_string()),
            },
            CommandSpec::CanvasSetName { usage } => match parse_canvas_name_set_args(args) {
                Some(invocation) => CommandRequest::Invocation(invocation),
                None => CommandRequest::Status(usage.to_string()),
            },
            CommandSpec::SplitPaneVertical => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::SplitPane {
                        axis: crate::kernel::SplitAxis::Vertical,
                    })
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::SplitPaneHorizontal => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::SplitPane {
                        axis: crate::kernel::SplitAxis::Horizontal,
                    })
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::FocusNextPane => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::FocusNextPane)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::FocusPreviousPane => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::FocusPreviousPane)
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::FocusPaneLeft => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::FocusPaneDirection {
                        direction: crate::kernel::PaneDirection::Left,
                    })
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::FocusPaneRight => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::FocusPaneDirection {
                        direction: crate::kernel::PaneDirection::Right,
                    })
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::FocusPaneUp => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::FocusPaneDirection {
                        direction: crate::kernel::PaneDirection::Up,
                    })
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::FocusPaneDown => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::FocusPaneDirection {
                        direction: crate::kernel::PaneDirection::Down,
                    })
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::ResizePaneLeft => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::ResizePaneDirection {
                        direction: crate::kernel::PaneDirection::Left,
                    })
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::ResizePaneRight => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::ResizePaneDirection {
                        direction: crate::kernel::PaneDirection::Right,
                    })
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::ResizePaneUp => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::ResizePaneDirection {
                        direction: crate::kernel::PaneDirection::Up,
                    })
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::ResizePaneDown => {
                if args.is_none() {
                    CommandRequest::Invocation(CommandInvocation::ResizePaneDirection {
                        direction: crate::kernel::PaneDirection::Down,
                    })
                } else {
                    CommandRequest::Status(format!("Unknown command: :{input}"))
                }
            }
            CommandSpec::PackageRun { usage } => match args {
                Some(args) => match parse_package_run_args(args) {
                    Some(invocation) => CommandRequest::Invocation(invocation),
                    None => CommandRequest::Status(usage.to_string()),
                },
                None => CommandRequest::Status(usage.to_string()),
            },
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }

    #[allow(dead_code)]
    pub fn scope(&self) -> CommandScope {
        self.scope
    }

    pub fn usage(&self) -> Option<&'static str> {
        self.spec.usage()
    }

    pub fn metadata(&self) -> CommandMetadata<'_> {
        CommandMetadata {
            name: &self.name,
            summary: &self.summary,
            scope: self.scope,
            usage: self.usage(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandMetadata<'a> {
    name: &'a str,
    summary: &'a str,
    scope: CommandScope,
    usage: Option<&'static str>,
}

impl<'a> CommandMetadata<'a> {
    pub fn name(&self) -> &str {
        self.name
    }

    pub fn summary(&self) -> &str {
        self.summary
    }

    pub fn scope(&self) -> CommandScope {
        self.scope
    }

    pub fn usage(&self) -> Option<&'static str> {
        self.usage
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandSpec {
    Placeholder,
    Quit,
    WindowNew,
    Help { usage: &'static str },
    EvalLua { usage: &'static str },
    WorkspaceSave { usage: &'static str },
    WorkspaceLoad { usage: &'static str },
    JobList,
    JobNext,
    JobPrevious,
    JobOpen,
    BufferStructuredCurrent,
    BufferStructuredOpen,
    BufferStructuredNext,
    BufferStructuredPrevious,
    BufferRecordCurrent,
    BufferRecordOpen,
    BufferRecordNext,
    BufferRecordPrevious,
    BufferTreeCurrent,
    BufferTreeOpen,
    BufferTreeNext,
    BufferTreePrevious,
    JobFocusOutput { usage: &'static str },
    JobDescribe { usage: &'static str },
    JobCancel { usage: &'static str },
    BufferDescribe,
    TerminalOpen { usage: &'static str },
    TerminalAppend { usage: &'static str },
    BufferNew { usage: &'static str },
    BrowserOpen { usage: &'static str },
    BrowserSetUrl { usage: &'static str },
    BrowserSetTitle { usage: &'static str },
    MediaOpen { usage: &'static str },
    MediaSetSource { usage: &'static str },
    CanvasOpen { usage: &'static str },
    CanvasSetName { usage: &'static str },
    SplitPaneVertical,
    SplitPaneHorizontal,
    FocusNextPane,
    FocusPreviousPane,
    FocusPaneLeft,
    FocusPaneRight,
    FocusPaneUp,
    FocusPaneDown,
    ResizePaneLeft,
    ResizePaneRight,
    ResizePaneUp,
    ResizePaneDown,
    PackageRun { usage: &'static str },
}

impl CommandSpec {
    pub const fn placeholder() -> Self {
        Self::Placeholder
    }

    pub const fn quit() -> Self {
        Self::Quit
    }

    pub const fn window_new() -> Self {
        Self::WindowNew
    }

    pub const fn help() -> Self {
        Self::Help {
            usage: "Usage: :help [command]",
        }
    }

    pub const fn eval_lua() -> Self {
        Self::EvalLua {
            usage: "Usage: :eval <lua code>",
        }
    }

    pub const fn workspace_save() -> Self {
        Self::WorkspaceSave {
            usage: "Usage: :workspace.save <path>",
        }
    }

    pub const fn workspace_load() -> Self {
        Self::WorkspaceLoad {
            usage: "Usage: :workspace.load <path>",
        }
    }

    pub const fn job_list() -> Self {
        Self::JobList
    }

    pub const fn job_next() -> Self {
        Self::JobNext
    }

    pub const fn job_previous() -> Self {
        Self::JobPrevious
    }

    pub const fn job_open() -> Self {
        Self::JobOpen
    }

    pub const fn buffer_structured_current() -> Self {
        Self::BufferStructuredCurrent
    }

    pub const fn buffer_structured_open() -> Self {
        Self::BufferStructuredOpen
    }

    pub const fn buffer_structured_next() -> Self {
        Self::BufferStructuredNext
    }

    pub const fn buffer_structured_previous() -> Self {
        Self::BufferStructuredPrevious
    }

    pub const fn buffer_record_current() -> Self {
        Self::BufferRecordCurrent
    }

    pub const fn buffer_record_open() -> Self {
        Self::BufferRecordOpen
    }

    pub const fn buffer_record_next() -> Self {
        Self::BufferRecordNext
    }

    pub const fn buffer_record_previous() -> Self {
        Self::BufferRecordPrevious
    }

    pub const fn buffer_tree_current() -> Self {
        Self::BufferTreeCurrent
    }

    pub const fn buffer_tree_open() -> Self {
        Self::BufferTreeOpen
    }

    pub const fn buffer_tree_next() -> Self {
        Self::BufferTreeNext
    }

    pub const fn buffer_tree_previous() -> Self {
        Self::BufferTreePrevious
    }

    pub const fn job_focus_output() -> Self {
        Self::JobFocusOutput {
            usage: "Usage: :job.focus-output [job-id]",
        }
    }

    pub const fn job_describe() -> Self {
        Self::JobDescribe {
            usage: "Usage: :job.describe [job-id]",
        }
    }

    pub const fn job_cancel() -> Self {
        Self::JobCancel {
            usage: "Usage: :job.cancel [job-id]",
        }
    }

    pub const fn buffer_describe() -> Self {
        Self::BufferDescribe
    }

    pub const fn terminal_open() -> Self {
        Self::TerminalOpen {
            usage: "Usage: :terminal.open <name>",
        }
    }

    pub const fn terminal_append() -> Self {
        Self::TerminalAppend {
            usage: "Usage: :terminal.append [buffer-id] <text>",
        }
    }

    pub const fn buffer_new() -> Self {
        Self::BufferNew {
            usage: "Usage: :buffer.new <name>",
        }
    }

    pub const fn browser_open() -> Self {
        Self::BrowserOpen {
            usage: "Usage: :browser.open <url>",
        }
    }

    pub const fn browser_set_url() -> Self {
        Self::BrowserSetUrl {
            usage: "Usage: :browser.url.set [buffer-id] <url>",
        }
    }

    pub const fn browser_set_title() -> Self {
        Self::BrowserSetTitle {
            usage: "Usage: :browser.title.set [buffer-id] <title>",
        }
    }

    pub const fn media_open() -> Self {
        Self::MediaOpen {
            usage: "Usage: :media.open <source>",
        }
    }

    pub const fn media_set_source() -> Self {
        Self::MediaSetSource {
            usage: "Usage: :media.source.set [buffer-id] <source>",
        }
    }

    pub const fn canvas_open() -> Self {
        Self::CanvasOpen {
            usage: "Usage: :canvas.open <name>",
        }
    }

    pub const fn canvas_set_name() -> Self {
        Self::CanvasSetName {
            usage: "Usage: :canvas.name.set [buffer-id] <name>",
        }
    }

    pub const fn split_pane_vertical() -> Self {
        Self::SplitPaneVertical
    }

    pub const fn split_pane_horizontal() -> Self {
        Self::SplitPaneHorizontal
    }

    pub const fn focus_next_pane() -> Self {
        Self::FocusNextPane
    }

    pub const fn focus_previous_pane() -> Self {
        Self::FocusPreviousPane
    }

    pub const fn focus_pane_left() -> Self {
        Self::FocusPaneLeft
    }

    pub const fn focus_pane_right() -> Self {
        Self::FocusPaneRight
    }

    pub const fn focus_pane_up() -> Self {
        Self::FocusPaneUp
    }

    pub const fn focus_pane_down() -> Self {
        Self::FocusPaneDown
    }

    pub const fn resize_pane_left() -> Self {
        Self::ResizePaneLeft
    }

    pub const fn resize_pane_right() -> Self {
        Self::ResizePaneRight
    }

    pub const fn resize_pane_up() -> Self {
        Self::ResizePaneUp
    }

    pub const fn resize_pane_down() -> Self {
        Self::ResizePaneDown
    }

    pub const fn package_run() -> Self {
        Self::PackageRun {
            usage: "Usage: :package.run <package> <command> [key=value ...]",
        }
    }

    pub const fn usage(self) -> Option<&'static str> {
        match self {
            Self::Placeholder | Self::Quit => None,
            Self::Help { usage }
            | Self::EvalLua { usage }
            | Self::WorkspaceSave { usage }
            | Self::WorkspaceLoad { usage }
            | Self::JobFocusOutput { usage }
            | Self::JobDescribe { usage }
            | Self::JobCancel { usage }
            | Self::TerminalOpen { usage }
            | Self::TerminalAppend { usage }
            | Self::BufferNew { usage }
            | Self::BrowserOpen { usage }
            | Self::BrowserSetUrl { usage }
            | Self::BrowserSetTitle { usage }
            | Self::MediaOpen { usage }
            | Self::MediaSetSource { usage }
            | Self::CanvasOpen { usage }
            | Self::CanvasSetName { usage } => Some(usage),
            Self::WindowNew => Some("Usage: :window.new"),
            Self::JobList => Some("Usage: :job.list"),
            Self::JobNext => Some("Usage: :job.next"),
            Self::JobPrevious => Some("Usage: :job.prev"),
            Self::JobOpen => Some("Usage: :job.open"),
            Self::BufferStructuredCurrent => Some("Usage: :buffer.structured.current"),
            Self::BufferStructuredOpen => Some("Usage: :buffer.structured.open"),
            Self::BufferStructuredNext => Some("Usage: :buffer.structured.next"),
            Self::BufferStructuredPrevious => Some("Usage: :buffer.structured.prev"),
            Self::BufferRecordCurrent => Some("Usage: :buffer.record.current"),
            Self::BufferRecordOpen => Some("Usage: :buffer.record.open"),
            Self::BufferRecordNext => Some("Usage: :buffer.record.next"),
            Self::BufferRecordPrevious => Some("Usage: :buffer.record.prev"),
            Self::BufferTreeCurrent => Some("Usage: :buffer.tree.current"),
            Self::BufferTreeOpen => Some("Usage: :buffer.tree.open"),
            Self::BufferTreeNext => Some("Usage: :buffer.tree.next"),
            Self::BufferTreePrevious => Some("Usage: :buffer.tree.prev"),
            Self::BufferDescribe => Some("Usage: :buffer.describe"),
            Self::SplitPaneVertical => Some("Usage: :pane.split.vertical"),
            Self::SplitPaneHorizontal => Some("Usage: :pane.split.horizontal"),
            Self::FocusNextPane => Some("Usage: :pane.next"),
            Self::FocusPreviousPane => Some("Usage: :pane.prev"),
            Self::FocusPaneLeft => Some("Usage: :pane.left"),
            Self::FocusPaneRight => Some("Usage: :pane.right"),
            Self::FocusPaneUp => Some("Usage: :pane.up"),
            Self::FocusPaneDown => Some("Usage: :pane.down"),
            Self::ResizePaneLeft => Some("Usage: :pane.resize.left"),
            Self::ResizePaneRight => Some("Usage: :pane.resize.right"),
            Self::ResizePaneUp => Some("Usage: :pane.resize.up"),
            Self::ResizePaneDown => Some("Usage: :pane.resize.down"),
            Self::PackageRun { usage } => Some(usage),
        }
    }
}

fn parse_package_run_args(args: &str) -> Option<CommandInvocation> {
    let mut parts = args.split_whitespace();
    let package = parts.next()?.to_string();
    let command = parts.next()?.to_string();
    let mut parsed_args = BTreeMap::new();

    for part in parts {
        let (key, value) = part.split_once('=')?;
        parsed_args.insert(key.to_string(), value.to_string());
    }

    Some(CommandInvocation::PackageRun {
        package,
        command,
        args: parsed_args,
    })
}

fn parse_browser_title_set_args(args: Option<&str>) -> Option<CommandInvocation> {
    parse_optional_buffer_target(args, |buffer_id, title| {
        CommandInvocation::BrowserSetTitle { buffer_id, title }
    })
}

fn parse_browser_url_set_args(args: Option<&str>) -> Option<CommandInvocation> {
    parse_optional_buffer_target(args, |buffer_id, url| CommandInvocation::BrowserSetUrl {
        buffer_id,
        url,
    })
}

fn parse_media_source_set_args(args: Option<&str>) -> Option<CommandInvocation> {
    parse_optional_buffer_target(args, |buffer_id, source| {
        CommandInvocation::MediaSetSource { buffer_id, source }
    })
}

fn parse_canvas_name_set_args(args: Option<&str>) -> Option<CommandInvocation> {
    parse_optional_buffer_target(args, |buffer_id, name| CommandInvocation::CanvasSetName {
        buffer_id,
        name,
    })
}

fn parse_optional_buffer_target(
    args: Option<&str>,
    build: impl FnOnce(Option<crate::kernel::BufferId>, String) -> CommandInvocation,
) -> Option<CommandInvocation> {
    let args = args?.trim();
    if args.is_empty() {
        return None;
    }

    if let Some((buffer_id, value)) = args.split_once(char::is_whitespace) {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }

        if let Ok(buffer_id) = buffer_id.parse::<u64>() {
            return Some(build(
                Some(crate::kernel::BufferId::new(buffer_id)),
                value.to_string(),
            ));
        }
    }

    Some(build(None, args.to_string()))
}

fn parse_terminal_append_args(args: Option<&str>) -> Option<CommandInvocation> {
    parse_optional_buffer_target(args, |buffer_id, text| CommandInvocation::TerminalAppend {
        buffer_id,
        text,
    })
}

fn parse_optional_job_id(args: Option<&str>) -> Option<Option<crate::kernel::JobId>> {
    let Some(args) = args else {
        return Some(None);
    };

    let args = args.trim();
    if args.is_empty() {
        return Some(None);
    }

    if args.contains(char::is_whitespace) {
        return None;
    }

    args.parse::<u64>()
        .ok()
        .map(crate::kernel::JobId::new)
        .map(Some)
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandScope {
    Global,
    Workspace,
    Pane,
    Buffer,
    Surface,
    Minibuffer,
}
