#![forbid(unsafe_code)]

//! SRPL compiler facade.
//!
//! The crate keeps its historical root-level re-exports for compatibility,
//! while also exposing professionalized module boundaries:
//! - [`procedure_compiler`] owns source-to-AST binding and IR lowering entry points.
//! - [`procedure_model`] owns AST, contract, cardinality, and IR data shapes.
//! - [`SrplDiagnostic`] and [`source_location`] own source spans and validation diagnostics.

mod binder {
    pub use andromeda_srpl_binder::bind_procedure;
}
pub mod definition_batch_bridge;
pub mod execution_adapter {
    pub use andromeda_srpl_execution_adapter::*;
}
pub mod interpreter {
    pub use andromeda_srpl_interpreter::*;
}
mod lowering;
pub mod optimizer {
    pub use andromeda_optimizer::srpl::{
        OptimizationLevel, OptimizerDecisionKind, OptimizerDiagnostic, OptimizerPipelineConfig,
        OptimizerPipelineResult, constant_fold, cost_model, diagnostics, function_fold, liveness,
        normalize, optimize_procedure_ir, optimize_procedure_ir_with_config, phase, pipeline,
        plan_choice, plan_kind, predicate_fold, predicate_pushdown, projection_pushdown,
        run_optimizer_pipeline, safety,
    };
}
pub mod procedure_compiler;
pub mod procedure_model;
pub mod procedure_resolver {
    pub use andromeda_procedure_runtime::procedure_resolver::{
        ProcedureResolveError, ProcedureResolveRequest, ProcedureResolveResponse,
        ProcedureResolveTarget, ProcedureResolver, SrplProcedureManifest,
    };
}

pub mod source_location {
    pub use andromeda_srpl_diagnostics::source_location::{SourceSpan, SrplSource};
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
