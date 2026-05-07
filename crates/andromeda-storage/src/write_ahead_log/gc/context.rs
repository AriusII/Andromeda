use andromeda_core::AndromedaResult;

use crate::Lsn;

use super::{ArchiveStatus, WalGcAuditEvent, WalGcCandidate};

/// WAL Garbage Collection context for identification and removal.
///
/// This trait abstracts the WAL manager and backup integration layer
/// to allow testing and future extensibility.
pub trait WalGcContext: Send + Sync {
    /// Identify candidates for garbage collection.
    ///
    /// Returns a sorted list (oldest first) of segments eligible for GC
    /// based on LSN thresholds and visibility constraints.
    fn identify_gc_candidates(
        &self,
        min_active_snapshot_lsn: Lsn,
    ) -> AndromedaResult<Vec<WalGcCandidate>>;

    /// Verify that a segment has been archived.
    ///
    /// Query the backup integration layer to confirm the segment
    /// is included in at least one completed backup.
    ///
    /// # Safety
    ///
    /// Returns `ArchiveStatus::Unknown` on any error — never fails on
    /// lookup timeout or transient backup integration issues.
    fn verify_archived(&self, candidate: &WalGcCandidate) -> AndromedaResult<ArchiveStatus>;

    /// Safely remove a segment from HotStore after archive verification.
    ///
    /// Preconditions:
    /// - Segment must be verified archived
    /// - Segment LSN must not overlap with recovery requirements
    /// - Segment must not contain in-flight transactions
    ///
    /// Postconditions (on success):
    /// - Segment file is deleted from HotStore
    /// - Segment is removed from in-memory directory
    /// - Audit event is emitted
    fn safe_remove_segment(&self, candidate: &WalGcCandidate) -> AndromedaResult<()>;

    /// Get the minimum active snapshot LSN.
    ///
    /// Used to determine which segments have become invisible
    /// to all active transactions.
    fn min_active_snapshot_lsn(&self) -> Lsn;

    /// Get the recovery boundary LSN from the latest manifest.
    ///
    /// Segments with creation_lsn <= this LSN may be needed by recovery
    /// and must not be garbage collected.
    fn required_wal_start_lsn(&self) -> Lsn;

    /// Emit an audit event for traceability.
    fn emit_audit_event(&self, event: WalGcAuditEvent) -> AndromedaResult<()>;
}
