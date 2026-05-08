//! Compatibility facade for physical backup artifact DTOs.

use andromeda_core::CatalogVersion;
use andromeda_observe::TraceId;

use crate::Lsn;

pub use super::backup_owner::{
    BackupArtifactCompatibilityEvidence, BackupArtifactDigest, BackupIncompleteTransactionPolicy,
    BackupResourceBounds,
};

pub type BackupCompatibility = super::backup_owner::BackupCompatibility<CatalogVersion>;
pub type BackupColdSnapshotArtifact = super::backup_owner::BackupColdSnapshotArtifact;
pub type BackupWalSegmentArtifact = super::backup_owner::BackupWalSegmentArtifact<Lsn>;
pub type BackupPhysicalArtifactSet = super::backup_owner::BackupPhysicalArtifactSet<Lsn>;
pub type BackupIncompleteTransactionBoundary =
    super::backup_owner::BackupIncompleteTransactionBoundary<Lsn>;
pub type BackupAuditTraceFields = super::backup_owner::BackupAuditTraceFields<TraceId>;
