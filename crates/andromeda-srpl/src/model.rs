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
    BoundSrplBodyPlan, BoundSrplOperationPlan, ExecutableProcedurePlan, MAX_SRPL_BODY_OPERATIONS,
    SrplAssignmentIr, SrplBusinessOperationIr, SrplBusinessOperationKindIr,
    SrplCatalogBindingEvidence, SrplEmitValueIr, SrplObjectBindingEvidence, SrplPredicateIr,
    SrplProcedureBodyIr, SrplProcedureContractMetadata, SrplProcedureIr, SrplResultStreamIr,
    SrplValueIr,
};
pub use crate::signature::{ProcedureSignature, ResultContract};
