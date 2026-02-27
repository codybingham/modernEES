//! Core domain crate for Modern EES.

pub mod parser;
pub mod props;
pub mod solver;
pub mod units;

pub use parser::{ast, diagnostic, parse_expression, parse_program};

pub use units::analyze_units;

pub use solver::{
    solve_program, solve_program_with_options, ConvergenceReport, ConvergenceStatus, SolveError,
    SolveOptions, SolveResult,
};
