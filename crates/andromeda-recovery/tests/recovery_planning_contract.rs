//! Recovery planning contract suite.
//!
//! These tests cover recovery-owned redo planning, WAL scan boundaries, and
//! trace evidence using direct owner crate imports.

#[path = "recovery_planning_contract/chain_validation.rs"]
mod chain_validation;
#[path = "recovery_planning_contract/manifest_redo.rs"]
mod manifest_redo;
#[path = "recovery_planning_contract/scan_trace.rs"]
mod scan_trace;
#[path = "recovery_planning_contract/support.rs"]
mod support;
