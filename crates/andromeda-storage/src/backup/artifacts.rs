//! Compatibility facade for physical backup artifact DTOs.

use andromeda_core::CatalogVersion;
use andromeda_observe::TraceId;

use crate::Lsn;

pub type BackupArtifactCompatibilityEvidence =
    andromeda_backup::BackupArtifactCompatibilityEvidence;
pub type BackupArtifactDigest = andromeda_backup::BackupArtifactDigest;
pub type BackupResourceBounds = andromeda_backup::BackupResourceBounds;
pub type BackupIncompleteTransactionPolicy = andromeda_backup::BackupIncompleteTransactionPolicy;
pub type BackupCompatibility = andromeda_backup::BackupCompatibility<CatalogVersion>;
pub type BackupColdSnapshotArtifact = andromeda_backup::BackupColdSnapshotArtifact;
pub type BackupIncompleteTransactionBoundary =
    andromeda_backup::BackupIncompleteTransactionBoundary<Lsn>;
pub type BackupPhysicalArtifactSet = andromeda_backup::BackupPhysicalArtifactSet<Lsn>;
pub type BackupWalSegmentArtifact = andromeda_backup::BackupWalSegmentArtifact<Lsn>;
pub type BackupAuditTraceFields = andromeda_backup::BackupAuditTraceFields<TraceId>;
