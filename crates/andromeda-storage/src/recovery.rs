mod coverage;
mod planning;
mod startup;
mod trace;

use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub use coverage::WalCoverageEvidence;
pub use planning::{
    ConceptualRedoPlan, RecoveryPlan, RedoRecordDecision, RedoRecordPlan, StartupMode,
};
pub use startup::{
    decide_startup, ObservedBoundary, StartupAcceptance, StartupAuditProjection, StartupDecision,
    StartupEvidence, StartupOutcome, StartupRejectionReason,
};
pub use trace::RecoveryTrace;

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
