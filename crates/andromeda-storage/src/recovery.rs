pub mod catalog_replay;
mod coverage;
mod planning;
mod startup;
mod trace;

use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub use catalog_replay::{CatalogSnapshot, replay_catalog_wal_records};
pub use coverage::WalCoverageEvidence;
pub use planning::{
    ConceptualRedoPlan, RecoveryPlan, RedoRecordDecision, RedoRecordPlan, StartupMode,
};
pub use startup::{
    ObservedBoundary, StartupAcceptance, StartupAuditProjection, StartupDecision, StartupEvidence,
    StartupOutcome, StartupRejectionReason, decide_startup,
};
pub use trace::RecoveryTrace;

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
