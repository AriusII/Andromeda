use andromeda_error::{AndromedaError, AndromedaErrorKind};

mod format_gate;
mod plan;
mod redo;
mod trace_projection;

pub use andromeda_manifest::format_version::StorageFormatFingerprint;
pub use format_gate::{
    PreRedoStorageFormatDecision, PreRedoStorageFormatGate, PreRedoStorageFormatRejection,
    RECOVERY_REQUIRED_STORAGE_FORMATS, RecoveryManifestView,
};
pub use plan::{ConceptualRedoPlan, RecoveryPlan};
pub use redo::{RedoRecordDecision, RedoRecordPlan};
pub use trace_projection::RecoveryTraceProjection;

fn recovery_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
