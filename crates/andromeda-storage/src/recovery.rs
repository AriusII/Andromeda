mod catalog_replay;
mod coverage;
mod fast_start;
mod forensic_start;
mod planning;
mod replay;
mod safe_start;
mod startup;
mod trace;
mod wal_replay;

use andromeda_error::{AndromedaError, AndromedaErrorKind};

pub use andromeda_recovery::{UndoChain, UndoChainsBuilder, UndoOperation, UndoRecord};
pub use catalog_replay::{
    CatalogReplayFromLsnReport, CatalogSnapshot, LsnBoundCatalogRecord, replay_catalog_from_lsn,
    replay_catalog_wal_records,
};
pub use coverage::WalCoverageEvidence;
pub use fast_start::{FastStartAcceptance, FastStartRejection, fast_start_from_manifest_and_scan};
pub use forensic_start::{
    ForensicAnomaly, ForensicAnomalyKind, ForensicAnomalyReport, ForensicStartAcceptance,
    forensic_start_from_manifest_and_scan,
};
pub use planning::{
    ConceptualRedoPlan, PreRedoStorageFormatDecision, PreRedoStorageFormatGate,
    PreRedoStorageFormatRejection, RECOVERY_REQUIRED_STORAGE_FORMATS, RecoveryPlan,
    RedoRecordDecision, RedoRecordPlan, StartupMode, StorageFormatFingerprint,
};
pub use replay::{
    HeapRedoPageState, HeapRedoSlotState, IndexRebuildRequiredEvidence, ReplayContext,
    ReplayOutcome, ReplayResult, replay_wal_record,
};
pub use safe_start::{
    SafeStartAcceptance, SafeStartInvariantReport, SafeStartTailDiscard,
    safe_start_from_manifest_and_scan, verify_safe_start_invariants,
};
pub use startup::{
    ObservedBoundary, StartupAcceptance, StartupAuditProjection, StartupDecision, StartupEvidence,
    StartupOutcome, StartupRejectionReason, decide_startup,
};
pub use trace::RecoveryTrace;
pub use wal_replay::{
    WalReplayReport, execute_redo_plan, execute_redo_plan_into_context, replay_wal_from_lsn,
    replay_wal_from_lsn_into_context,
};

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
