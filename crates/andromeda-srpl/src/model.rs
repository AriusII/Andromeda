//! SRPL model and contract API.
//!
//! This module groups durable compiler data shapes: parsed AST nodes, bound
//! procedure contracts, result cardinality, and the canonical narrow IR.

pub use crate::ast::{FieldAst, ProcedureAst, ResultStreamAst, Spanned};
pub use crate::cardinality::Cardinality;
pub use crate::ir::{SrplProcedureIr, SrplResultStreamIr};
pub use crate::signature::{ProcedureSignature, ResultContract};
