#[cfg_attr(not(test), allow(dead_code))]
pub mod buffer;
#[cfg_attr(not(test), allow(dead_code))]
pub mod capability;
#[cfg_attr(not(test), allow(dead_code))]
pub mod command;
#[cfg_attr(not(test), allow(dead_code))]
pub mod ids;
#[cfg_attr(not(test), allow(dead_code))]
pub mod job;
#[cfg_attr(not(test), allow(dead_code))]
pub mod keymap;
#[cfg_attr(not(test), allow(dead_code))]
pub mod messages;
pub mod minibuffer;
#[cfg_attr(not(test), allow(dead_code))]
pub mod pane;
#[cfg_attr(not(test), allow(dead_code))]
pub mod pane_tree;
#[cfg_attr(not(test), allow(dead_code))]
pub mod selection;
#[cfg_attr(not(test), allow(dead_code))]
pub mod workspace;

#[allow(unused_imports)]
pub use buffer::{
    BrowserContent, Buffer, BufferContent, BufferKind, CanvasContent, MediaContent, RecordsContent,
    TerminalContent, TextContent, TextLine, TreeContent, TreeNode,
};
#[allow(unused_imports)]
pub use capability::{Capability, CapabilityDomain, CapabilityRegistry};
#[allow(unused_imports)]
pub use command::{
    Command, CommandData, CommandEffect, CommandInvocation, CommandMetadata, CommandRegistry,
    CommandRequest, CommandResult, CommandScope, CommandSpec, PackageInvocationRequest,
};
#[allow(unused_imports)]
pub use ids::{BufferId, IdAllocator, JobId, PaneId, WorkspaceId};
#[allow(unused_imports)]
pub use job::{Job, JobKind, JobOwner, JobRegistry, JobStatus};
#[allow(unused_imports)]
pub use keymap::{KeyChord, KeyCodeRepr, KeyModifiersRepr, KeymapLookup, KeymapRegistry};
#[allow(unused_imports)]
pub use messages::{Message, MessageLevel, MessageLog};
pub use minibuffer::{Minibuffer, MinibufferMode};
#[allow(unused_imports)]
pub use pane::{Pane, PanePresentation, Viewport};
#[allow(unused_imports)]
pub use pane_tree::{PaneDirection, PaneNode, PaneTree, SplitAxis};
#[allow(unused_imports)]
pub use selection::{
    RecordSelection, Selection, SurfaceSelection, TextRange, TextSelection, TreeSelection,
};
pub use workspace::Workspace;
