//! Storage compatibility adapter for recovery-owned WAL replay.
//!
//! Concrete durable replay handlers live in `andromeda-recovery`. Storage keeps
//! this module to preserve existing public imports while storage-specific redo
//! orchestration continues to validate publication into storage state.

pub use andromeda_recovery::{
    HeapRedoPageState, HeapRedoSlotState, IndexRebuildRequiredEvidence,
    ManifestSwitchRecoveryTrace, ReplayContext, ReplayOutcome, ReplayResult, replay_wal_record,
};

pub(crate) use andromeda_recovery::replay_wal_record_result;
