pub mod catalog_replay;
mod coverage;
mod planning;
pub mod replay;
mod startup;
mod trace;
pub mod undo;

use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub use catalog_replay::{CatalogSnapshot, replay_catalog_wal_records};
pub use coverage::WalCoverageEvidence;
pub use planning::{
    ConceptualRedoPlan, PreRedoStorageFormatDecision, PreRedoStorageFormatGate,
    PreRedoStorageFormatRejection, RECOVERY_REQUIRED_STORAGE_FORMATS, RecoveryPlan,
    RedoRecordDecision, RedoRecordPlan, StartupMode, StorageFormatFingerprint,
};
pub use replay::{ReplayContext, ReplayOutcome, ReplayResult, replay_wal_record};
pub use startup::{
    ObservedBoundary, StartupAcceptance, StartupAuditProjection, StartupDecision, StartupEvidence,
    StartupOutcome, StartupRejectionReason, decide_startup,
};
pub use trace::RecoveryTrace;
pub use undo::{UndoChain, UndoChainsBuilder, UndoOperation, UndoRecord};

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
