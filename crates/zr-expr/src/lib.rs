//! Standalone expression engine for zacor packages.
//!
//! This crate owns the expression language used by packages like `where`
//! and `mutate`. It intentionally stays separate from `zacor-package`,
//! which owns protocol/runtime concerns.

mod eval;
mod lexer;
mod parser;

pub use eval::{eval_predicate, eval_value};
pub use parser::{Expr, Predicate};
