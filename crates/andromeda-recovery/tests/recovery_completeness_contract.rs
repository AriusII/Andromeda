//! Recovery completeness contract tests.
//!
//! These tests exercise recovery-owned WAL replay classification and handler
//! gates without depending on storage compatibility reexports.

#[path = "recovery_completeness_contract/support.rs"]
mod support;

#[path = "recovery_completeness_contract/classification.rs"]
mod classification;
#[path = "recovery_completeness_contract/manifest_switch.rs"]
mod manifest_switch;
#[path = "recovery_completeness_contract/redo_boundary.rs"]
mod redo_boundary;
#[path = "recovery_completeness_contract/wal_chain_validation.rs"]
mod wal_chain_validation;
