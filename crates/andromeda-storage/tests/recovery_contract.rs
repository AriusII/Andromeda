//! Storage recovery format-gate contract suite.
//!
//! Pure recovery planning contracts have moved to `andromeda-recovery`.
//! Storage keeps pre-redo storage-format compatibility gates because they
//! depend on storage subformat fingerprints.

#[path = "recovery_contract/format_gate.rs"]
mod format_gate;
#[path = "recovery_contract/support.rs"]
mod support;
