pub mod ast;
pub mod diagnostic;
mod lexer;
mod parser_impl;

pub use parser_impl::parse_program;

#[cfg(test)]
mod tests;
