#![forbid(unsafe_code)]
#![doc = r#"
C5 durability crate for Andromeda backup artifact lifecycle.

This crate owns backup artifact creation, immutability gates, and retention policies.
Backup artifacts are immutable after creation and advisory retention only;
they are not truth sources. Recovery truth comes from durable WAL and manifests.

C5 invariants:
- Backup artifacts must be marked immutable after creation.
- Writes to immutable backup artifacts must be rejected.
- Backup artifact digests (SHA256, CRC64) must be nonzero and validated.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
"#]

pub mod artifacts;
pub mod checkpoint_manager;
pub mod error;
pub mod immutability;
pub mod physical_plan;
pub mod plan;
pub mod primitives;
pub mod types;

pub use artifacts::{
    BackupArtifactCompatibilityEvidence, BackupArtifactDigest, BackupAuditTraceFields,
    BackupColdSnapshotArtifact, BackupCompatibility, BackupIncompleteTransactionBoundary,
    BackupIncompleteTransactionPolicy, BackupPhysicalArtifactSet, BackupResourceBounds,
    BackupWalSegmentArtifact,
};
pub use checkpoint_manager::{
    BackupCheckpoint, BackupCheckpointManager, BackupCheckpointMetadata, BackupRecoveryInfo,
};
pub use error::{BackupResult, BackupValidationError};
pub use immutability::{BackupArtifactImmutabilityGuard, ImmutableBackupArtifact};
pub use physical_plan::{BackupPhase, BackupPhysicalPlan, PhysicalPageScan, SegmentPlan};
pub use plan::{BackupManifest, validate_wal_segment_chain};
pub use primitives::{
    BackupCatalogVersion, BackupLsn, BackupTraceId, CatalogVersion, Lsn, TraceId,
    WAL_FORMAT_VERSION,
};
pub use types::{
    BACKUP_PHYSICAL_PLAN_VERSION_V0, BACKUP_SUPPORTED_STORAGE_FORMAT_VERSION_V0, BackupId,
    ColdSnapshotBoundary, WalArchiveRange,
};

#[cfg(test)]
mod immutability_tests;
