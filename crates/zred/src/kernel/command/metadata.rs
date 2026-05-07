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
            CommandSpec::BufferNew { usage } => match args {
                Some(name) if !name.trim().is_empty() => {
                    CommandRequest::Invocation(CommandInvocation::BufferNew {
                        name: name.trim().to_string(),
                    })
                }
                _ => CommandRequest::Status(usage.to_string()),
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
    Help { usage: &'static str },
    EvalLua { usage: &'static str },
    BufferNew { usage: &'static str },
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

    pub const fn buffer_new() -> Self {
        Self::BufferNew {
            usage: "Usage: :buffer.new <name>",
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
            Self::Help { usage } | Self::EvalLua { usage } | Self::BufferNew { usage } => {
                Some(usage)
            }
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
