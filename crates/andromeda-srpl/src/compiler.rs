//! Compiler-stage API for SRPL.
//!
//! This module groups the narrow SRPL compiler pipeline by phase. Root-level
//! re-exports remain available for compatibility, but new code should prefer
//! importing lexer/parser/binder/lowering entry points from this boundary.

pub use crate::binder::{BoundProcedure, bind_procedure};
pub use crate::lexer::{Token, TokenKind, lex};
pub use crate::lowering::{
    compile_narrow_procedure_contract_candidate, compile_narrow_procedure_signature,
    inventory_reserve_stock_body_ir, lower_body_ast, lower_bound_procedure,
    lower_ir_to_contract_candidate,
};
pub use crate::parser::parse_procedure_signature;
