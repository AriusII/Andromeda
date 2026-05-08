//! Compatibility facade for SRPL optimizer APIs.
//!
//! The implementation lives in `andromeda-optimizer::srpl`; this module keeps
//! the historical `andromeda_srpl::optimizer::*` paths stable for compiler
//! callers and tests.

pub use andromeda_optimizer::srpl::{
    OptimizationLevel, OptimizerDecisionKind, OptimizerDiagnostic, OptimizerPipelineConfig,
    OptimizerPipelineResult, constant_fold, cost_model, diagnostics, function_fold, liveness,
    normalize, optimize_procedure_ir, optimize_procedure_ir_with_config, phase, pipeline,
    plan_choice, plan_kind, predicate_fold, predicate_pushdown, projection_pushdown,
    run_optimizer_pipeline, safety,
};
