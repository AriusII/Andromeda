//! Compiler-stage API for SRPL.
//!
//! This module groups the narrow SRPL compiler pipeline by phase. Root-level
//! re-exports remain available for compatibility, but new code should prefer
//! importing lexer/parser/binder/lowering entry points from this boundary.

pub use crate::binder::{bind_procedure, BoundProcedure};
pub use crate::lexer::{lex, Token, TokenKind};
pub use crate::lowering::{
    compile_narrow_procedure_contract_candidate, compile_narrow_procedure_signature,
    lower_bound_procedure, lower_ir_to_contract_candidate,
};
pub use crate::parser::parse_procedure_signature;
