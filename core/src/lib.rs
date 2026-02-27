//! Core domain crate for Modern EES.

pub mod parser;

pub use parser::{ast, diagnostic, parse_expression, parse_program};
