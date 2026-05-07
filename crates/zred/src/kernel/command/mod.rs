mod dispatch;
mod metadata;
mod parse;
mod registry;
#[cfg(test)]
mod tests;
mod types;

pub use metadata::{Command, CommandMetadata, CommandScope, CommandSpec};
pub use registry::CommandRegistry;
pub use types::{
    CommandData, CommandEffect, CommandInvocation, CommandRequest, CommandResult,
    PackageInvocationRequest,
};
