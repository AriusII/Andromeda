//! Compatibility facade for restore orchestration contracts.
//!
//! The canonical restore/PITR orchestration DTOs and pure validators live in
//! `andromeda-restore`. Storage keeps this module as the legacy import path.

pub use andromeda_restore::{
    RecoveryStage, RestoreArtifactPreflight, RestoreAuditTrace, RestoreBackupManifest,
    RestoreCompletion, RestoreOrchestration, RestoreValidationPolicy, WalSegmentToReplay,
    compute_restore_checksum, plan_replay_segments, validate_restore_artifact_preflight,
    validate_restore_prerequisites,
};
