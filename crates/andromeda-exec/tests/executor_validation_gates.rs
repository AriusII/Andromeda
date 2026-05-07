//! Executor-layer validation gates for H1-SRPL-EXEC-007.
//!
//! These gates cover SRPL dispatcher construction, pre-transaction request
//! validation, cataloged local dispatch coexistence, deterministic errors,
//! result metadata extraction, and plan validation.

#[path = "executor_validation_gates/support.rs"]
mod support;

#[path = "executor_validation_gates/adapter_contract.rs"]
mod adapter_contract;
#[path = "executor_validation_gates/dispatch_paths.rs"]
mod dispatch_paths;
#[path = "executor_validation_gates/error_boundary.rs"]
mod error_boundary;
#[path = "executor_validation_gates/plan_validation.rs"]
mod plan_validation;
#[path = "executor_validation_gates/result_metadata.rs"]
mod result_metadata;
#[path = "executor_validation_gates/summary.rs"]
mod summary;
