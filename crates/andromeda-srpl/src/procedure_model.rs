//! SRPL model and contract API.
//!
//! This module groups durable compiler data shapes: parsed AST nodes, bound
//! procedure contracts, result cardinality, and the canonical narrow IR.

pub use andromeda_srpl_ast::{
    BusinessOperationAst, BusinessOperationKindAst, FieldAst, ProcedureAst, ProcedureBodyAst,
    ResultStreamAst, Spanned,
};
pub use andromeda_srpl_cardinality::Cardinality;
pub use andromeda_srpl_ir::{
    ArithOp, BoundSrplBodyPlan, BoundSrplOperationPlan, ConstantLiteral, ExecutableProcedurePlan,
    MAX_EXPR_DEPTH, MAX_SRPL_BODY_OPERATIONS, SrplAssignmentIr, SrplBusinessOperationIr,
    SrplBusinessOperationKindIr, SrplCatalogBindingEvidence, SrplEmitValueIr,
    SrplObjectBindingEvidence, SrplPredicateIr, SrplProcedureBodyIr, SrplProcedureContractMetadata,
    SrplProcedureIr, SrplResultStreamIr, SrplValueIr,
};
pub use andromeda_srpl_ir::{ProcedureSignature, ResultContract};
