//! Module tree for the Z language implementation.
//!
//! ```text
//! lexer       source text -> tokens
//! ast         node definitions
//! parser      tokens -> AST
//! compiler    AST -> analyzed module
//! value       runtime values
//! vm          tree-walking evaluator
//! builtins    built-in functions and methods
//! stdlib      standard library modules
//! ```

pub mod ast;
pub mod builtins;
pub mod compiler;
pub mod error;
pub mod lexer;
pub mod native;
pub mod parser;
pub mod stdlib;
pub mod value;
pub mod vm;

pub use error::{ZError, ZResult};
pub use vm::Vm;
