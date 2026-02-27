//! Core domain crate for Modern EES.

pub mod param_table;
pub mod parser;
pub mod props;
pub mod solver;
pub mod units;

pub use parser::{ast, diagnostic, parse_expression, parse_program};

pub use units::analyze_units;

pub use solver::{
    evaluate_expression_string, solve_program, solve_program_with_options,
    solve_program_with_options_and_fixed, ConvergenceReport, ConvergenceStatus, SolveError,
    SolveOptions, SolveResult,
};
