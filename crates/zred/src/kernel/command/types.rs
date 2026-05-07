use crate::kernel::ids::BufferId;
use crate::kernel::ids::JobId;
use crate::kernel::pane_tree::{PaneDirection, SplitAxis};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandResult {
    effects: Vec<CommandEffect>,
    data: Option<CommandData>,
    error: Option<String>,
}

impl CommandResult {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_effect(effect: CommandEffect) -> Self {
        Self {
            effects: vec![effect],
            data: None,
            error: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_data(data: CommandData) -> Self {
        Self {
            effects: Vec::new(),
            data: Some(data),
            error: None,
        }
    }

    pub fn with_data_and_effect(data: CommandData, effect: CommandEffect) -> Self {
        Self {
            effects: vec![effect],
            data: Some(data),
            error: None,
        }
    }

    pub fn with_error(error: impl Into<String>) -> Self {
        Self {
            effects: Vec::new(),
            data: None,
            error: Some(error.into()),
        }
    }

    #[allow(dead_code)]
    pub fn push(&mut self, effect: CommandEffect) {
        self.effects.push(effect);
    }

    pub fn effects(&self) -> &[CommandEffect] {
        &self.effects
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn data(&self) -> Option<&CommandData> {
        self.data.as_ref()
    }

    pub fn into_parts(self) -> (Vec<CommandEffect>, Option<CommandData>, Option<String>) {
        (self.effects, self.data, self.error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandEffect {
    SetStatus(String),
    Quit,
    EvalLua(String),
    InvokePackage(PackageInvocationRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandData {
    BufferCreated { buffer_id: BufferId },
    PackageJobStarted { job_id: JobId, buffer_id: BufferId },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandInvocation {
    Quit,
    Help {
        name: Option<String>,
    },
    EvalLua {
        script: String,
    },
    BufferNew {
        name: String,
    },
    SplitPane {
        axis: SplitAxis,
    },
    FocusNextPane,
    FocusPreviousPane,
    FocusPaneDirection {
        direction: PaneDirection,
    },
    ResizePaneDirection {
        direction: PaneDirection,
    },
    PackageRun {
        package: String,
        command: String,
        args: BTreeMap<String, String>,
    },
    BufferAppend {
        buffer_id: BufferId,
        text: String,
    },
    BufferSetContents {
        buffer_id: BufferId,
        text: String,
    },
    BufferFocus {
        buffer_id: BufferId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandRequest {
    Invocation(CommandInvocation),
    Status(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageInvocationRequest {
    pub job_id: JobId,
    pub buffer_id: BufferId,
    pub package: String,
    pub command: String,
    pub args: BTreeMap<String, String>,
}
