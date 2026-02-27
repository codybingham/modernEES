//! Core domain crate for Modern EES.

pub mod parser;
pub mod props;
pub mod units;

pub use parser::{ast, diagnostic, parse_expression, parse_program};

pub use units::analyze_units;
