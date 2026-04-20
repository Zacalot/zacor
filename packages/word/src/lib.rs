zacor_package::include_args!();

pub mod models;
pub mod parser;
pub mod wordnet;
mod commands;
mod sentence;

pub use commands::{cmd_domain, cmd_lookup, cmd_pattern, cmd_random, cmd_related, cmd_sentence};
