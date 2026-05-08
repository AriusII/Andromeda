//! SRPL optimizer passes and pipeline.
//!
//! This module owns SRPL rewrite, projection, cost, plan choice, and pipeline
//! logic. The `andromeda-srpl` crate re-exports this module as a compatibility
//! facade for existing compiler callers.

pub mod constant_fold;
pub mod cost_model;
pub mod diagnostics;
pub mod function_fold;
pub mod liveness;
pub mod normalize;
pub mod phase;
pub mod pipeline;
pub mod plan_choice;
pub mod plan_kind;
pub mod predicate_fold;
pub mod predicate_pushdown;
pub mod projection_pushdown;
pub mod safety;

pub use diagnostics::{OptimizerDecisionKind, OptimizerDiagnostic};
pub use pipeline::{
    OptimizationLevel, OptimizerPipelineConfig, OptimizerPipelineResult, optimize_procedure_ir,
    optimize_procedure_ir_with_config, run_optimizer_pipeline,
};
