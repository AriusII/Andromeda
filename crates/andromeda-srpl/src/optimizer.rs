//! SRPL optimizer facade.
//!
//! Keep pass contracts in the modules that enforce them; this file only
//! declares the public optimizer module tree.

pub mod constant_fold;
pub mod cost_model;
pub mod function_fold;
pub mod liveness;
pub mod normalize;
pub mod phase;
pub mod plan_choice;
pub mod plan_kind;
pub mod predicate_fold;
pub mod predicate_pushdown;
pub mod projection_pushdown;
