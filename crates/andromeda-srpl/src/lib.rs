#![forbid(unsafe_code)]

//! SRPL compiler facade.
//!
//! The crate keeps its historical root-level re-exports for compatibility,
//! while also exposing professionalized module boundaries:
//! - [`procedure_compiler`] owns source-to-AST binding and IR lowering entry points.
//! - [`procedure_model`] is a compatibility module over AST, cardinality, and IR owners.
//! - [`SrplDiagnostic`] and [`source_location`] re-export diagnostics-owner source spans.

pub mod binder;
pub mod definition_batch_bridge;
pub mod execution_adapter;
pub mod interpreter;
mod lowering;
pub mod optimizer;
pub mod procedure_compiler;
pub mod procedure_model {
    pub use andromeda_srpl_ast::{
        BusinessOperationAst, BusinessOperationKindAst, FieldAst, ProcedureAst, ProcedureBodyAst,
        ResultStreamAst, Spanned,
    };
    pub use andromeda_srpl_cardinality::Cardinality;
    pub use andromeda_srpl_ir::{
        ArithOp, BoundSrplBodyPlan, BoundSrplOperationPlan, ConstantLiteral,
        ExecutableProcedurePlan, MAX_EXPR_DEPTH, MAX_SRPL_BODY_OPERATIONS, ProcedureSignature,
        ResultContract, SrplAssignmentIr, SrplBusinessOperationIr, SrplBusinessOperationKindIr,
        SrplCatalogBindingEvidence, SrplEmitValueIr, SrplObjectBindingEvidence, SrplPredicateIr,
        SrplProcedureBodyIr, SrplProcedureContractMetadata, SrplProcedureIr, SrplResultStreamIr,
        SrplValueIr,
    };
}
pub mod procedure_resolver {
    pub use andromeda_procedure_runtime::procedure_resolver::{
        ProcedureResolveError, ProcedureResolveRequest, ProcedureResolveResponse,
        ProcedureResolveTarget, ProcedureResolver, SrplProcedureManifest,
    };
}

pub mod source_location {
    pub use andromeda_srpl_diagnostics::{SourceSpan, SrplSource};
}

pub use andromeda_srpl_ast::{
    BusinessOperationAst, BusinessOperationKindAst, FieldAst, ProcedureAst, ProcedureBodyAst,
    ResultStreamAst, Spanned,
};
pub use andromeda_srpl_cardinality::Cardinality;
pub use andromeda_srpl_diagnostics::{
    DiagnosticPhase, ForbiddenConstruct, ForbiddenConstructHit, SourceSpan, SrplDiagnostic,
    SrplSource,
};
pub use andromeda_srpl_lexer::{Token, TokenKind, lex};
pub use andromeda_srpl_parser::parse_procedure_signature;
pub use definition_batch_bridge::*;
pub use procedure_compiler::*;
pub use procedure_model::*;
