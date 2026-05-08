use andromeda_core::AndromedaResult;

use crate::Lsn;

use super::{ArchiveStatus, WalGcAuditEvent, WalGcCandidate};

/// WAL Garbage Collection context for identification and removal.
pub trait WalGcContext: Send + Sync {
    /// Identify candidates for garbage collection.
    fn identify_gc_candidates(
        &self,
        min_active_snapshot_lsn: Lsn,
    ) -> AndromedaResult<Vec<WalGcCandidate>>;

    /// Verify that a segment has been archived.
    fn verify_archived(&self, candidate: &WalGcCandidate) -> AndromedaResult<ArchiveStatus>;

    /// Safely remove a segment after archive verification.
    fn safe_remove_segment(&self, candidate: &WalGcCandidate) -> AndromedaResult<()>;

    /// Get the minimum active snapshot LSN.
    fn min_active_snapshot_lsn(&self) -> Lsn;

    /// Get the recovery boundary LSN from the latest manifest.
    fn required_wal_start_lsn(&self) -> Lsn;

    /// Emit an audit event for traceability.
    fn emit_audit_event(&self, event: WalGcAuditEvent) -> AndromedaResult<()>;
}
