zacor_package::include_args!();

mod commands;
pub mod models;
pub mod parser;
mod sentence;
pub mod wordnet;

pub use commands::{cmd_domain, cmd_lookup, cmd_pattern, cmd_random, cmd_related, cmd_sentence};
