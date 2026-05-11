#![forbid(unsafe_code)]

//! Bounded Andromeda optimizer ownership contracts.
//!
//! This crate owns runtime-free optimizer policy and decision evidence for
//! plan selection inputs. It does not compile SRPL, store executable plans, or
//! let statistics, benchmark, ScenarioEvidence, learned output, or GPU output
//! select a plan alone.

pub mod columnar_pruning;
mod error;
mod plan_decision;
mod policy;
pub mod srpl;

pub use columnar_pruning::{
    ColumnScanPredicate, ColumnarPruningDecision, evaluate_columnar_pruning,
};
pub use error::OptimizerError;
pub use plan_decision::{
    OptimizerPlanDecision, OptimizerPlanReason, evaluate_optimizer_plan_inputs,
};
pub use policy::OptimizerPolicy;
