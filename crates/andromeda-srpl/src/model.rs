//! SRPL model and contract API.
//!
//! This module groups durable compiler data shapes: parsed AST nodes, bound
//! procedure contracts, result cardinality, and the canonical narrow IR.

pub use crate::ast::{
    BusinessOperationAst, BusinessOperationKindAst, FieldAst, ProcedureAst, ProcedureBodyAst,
    ResultStreamAst, Spanned,
};
pub use crate::cardinality::Cardinality;
pub use crate::ir::{
    MAX_SRPL_BODY_OPERATIONS, SrplAssignmentIr, SrplBusinessOperationIr,
    SrplBusinessOperationKindIr, SrplEmitValueIr, SrplPredicateIr, SrplProcedureBodyIr,
    SrplProcedureContractMetadata, SrplProcedureIr, SrplResultStreamIr, SrplValueIr,
};
pub use crate::signature::{ProcedureSignature, ResultContract};
