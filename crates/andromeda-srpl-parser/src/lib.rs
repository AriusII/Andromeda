#![forbid(unsafe_code)]

//! SRPL lexer and parser.
//!
//! This crate owns tokenization and syntax parsing only. It does not bind
//! catalog objects, produce executable plans, execute runtime behavior, or
//! depend on catalog storage.

mod parser;

const MAX_SRPL_BODY_OPERATIONS: usize = 16;

pub mod source_location {
    pub use andromeda_srpl_diagnostics::source_location::SrplSource;
    pub use andromeda_srpl_lexer::source_location::SourceSpan;
}

pub use andromeda_srpl_ast::*;
pub use andromeda_srpl_cardinality::Cardinality;
pub use andromeda_srpl_diagnostics::{
    DiagnosticPhase, ForbiddenConstruct, ForbiddenConstructHit, SrplDiagnostic,
};
pub use andromeda_srpl_lexer::{Token, TokenKind, lex};
pub use parser::parse_procedure_signature;
pub use source_location::{SourceSpan, SrplSource};
