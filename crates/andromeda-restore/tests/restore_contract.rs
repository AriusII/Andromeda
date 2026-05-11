//! F6 Restore and PITR contract tests.
//!
//! The test target is split by restore contract family:
//!
//! - PITR target and manifest prerequisite validation
//! - WAL replay planning and checkpoint reconstruction
//! - restore checksum and audit trace binding
//! - orchestration policy gates
//! - file-backed restore artifact preflight

#[path = "restore_contract/checksum_audit.rs"]
mod checksum_audit;
#[path = "restore_contract/orchestration.rs"]
mod orchestration;
#[path = "restore_contract/pitr_validation.rs"]
mod pitr_validation;
#[path = "restore_contract/preflight.rs"]
mod preflight;
#[path = "restore_contract/replay_planning.rs"]
mod replay_planning;
#[path = "restore_contract/restore_drill_proof.rs"]
mod restore_drill_proof;
#[path = "restore_contract/restore_plan_v0.rs"]
mod restore_plan_v0;
#[path = "restore_contract/support.rs"]
mod support;
