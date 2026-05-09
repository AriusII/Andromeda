//! HREDOV1 heap row redo replay contract suite.
//!
//! These tests cover the recovery-local heap apply target. They deliberately do
//! not claim durable page-store writeback; that bridge remains a later boundary.
//! The suite is split by direct row replay, snapshot hydration, and redo-plan
//! execution so each crash/replay contract stays focused.

#[path = "recovery_replay_heap_redo_contract/redo_plan.rs"]
mod redo_plan;
#[path = "recovery_replay_heap_redo_contract/row_replay.rs"]
mod row_replay;
#[path = "recovery_replay_heap_redo_contract/snapshot_hydration.rs"]
mod snapshot_hydration;
#[path = "recovery_replay_heap_redo_contract/support.rs"]
mod support;
