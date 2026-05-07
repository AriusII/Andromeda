//! Recovery planning contract suite.
//!
//! The suite is split by durable state evidence:
//!
//! - manifest and transaction replay boundaries
//! - WAL scan tail boundaries and recovery trace evidence
//! - LSN chain validation failures
//! - pre-redo storage-format gates

#[path = "recovery_contract/chain_validation.rs"]
mod chain_validation;
#[path = "recovery_contract/format_gate.rs"]
mod format_gate;
#[path = "recovery_contract/manifest_redo.rs"]
mod manifest_redo;
#[path = "recovery_contract/scan_trace.rs"]
mod scan_trace;
#[path = "recovery_contract/support.rs"]
mod support;
