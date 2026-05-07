#![forbid(unsafe_code)]

//! SRPL lexer and parser.
//!
//! This crate owns tokenization and syntax parsing only. It does not bind
//! catalog objects, produce executable plans, execute runtime behavior, or
//! depend on catalog storage.

mod lexer;
mod parser;

const MAX_SRPL_BODY_OPERATIONS: usize = 16;

pub use andromeda_srpl_ast::*;
pub use andromeda_srpl_cardinality::Cardinality;
pub use andromeda_srpl_diagnostics::*;
pub use lexer::{Token, TokenKind, lex};
pub use parser::parse_procedure_signature;
