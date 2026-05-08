#![forbid(unsafe_code)]

pub mod artifacts;
pub mod error;
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
pub use error::{BackupResult, BackupValidationError};
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
