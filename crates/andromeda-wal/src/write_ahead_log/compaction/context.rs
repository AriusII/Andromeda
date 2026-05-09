use andromeda_error::AndromedaResult;

use crate::WalRecord;

use super::{FragmentationMetrics, WalCompactionAuditEvent};

/// Context trait for integrating compaction with WAL manager and snapshot registry.
///
/// Implementations must provide methods to:
/// 1. Identify segments and their metrics
/// 2. Read records from a segment
/// 3. Determine record liveness (visibility to active snapshots)
/// 4. Rewrite segment to new location
/// 5. Atomically swap old ↔ new
pub trait CompactionContext: Send + Sync {
    /// Identify segments exceeding fragmentation threshold.
    ///
    /// Returns metrics for all segments, filtered by caller-provided threshold.
    fn identify_fragmented_segments(
        &self,
        threshold_ratio: f64,
    ) -> AndromedaResult<Vec<FragmentationMetrics>>;

    /// Read all records from a segment.
    ///
    /// # Errors
    ///
    /// Returns `Storage` error if segment cannot be read or is corrupted.
    fn read_segment_records(&self, segment_id: u64) -> AndromedaResult<Vec<WalRecord>>;

    /// Determine if a record should be kept during compaction.
    ///
    /// A record is "live" if:
    /// - Its LSN is visible to any active snapshot, OR
    /// - Its transaction is uncommitted, OR
    /// - Its segment is marked as undoable for recovery
    ///
    /// Returns `true` if record should be kept, `false` if it can be discarded.
    fn should_keep_record(&self, record: &WalRecord) -> AndromedaResult<bool>;

    /// Write a compacted segment with new segment_id to temporary location.
    ///
    /// Returns the new segment_id and byte size of the compacted segment.
    ///
    /// # Errors
    ///
    /// Returns `Storage` error if write fails. Temporary segment should be abandoned
    /// by caller on error.
    fn write_compacted_segment(
        &self,
        original_segment_id: u64,
        records: &[WalRecord],
    ) -> AndromedaResult<(u64, u64)>; // (new_segment_id, bytes_written)

    /// Atomically swap old segment with new segment (HotStore → old location).
    ///
    /// On failure, old segment must remain intact and usable.
    ///
    /// # Errors
    ///
    /// Returns `Storage` error if swap fails. On error, caller should abandon
    /// the new segment and rely on old segment for recovery.
    fn swap_segment(&self, old_segment_id: u64, new_segment_id: u64) -> AndromedaResult<()>;

    /// Emit an audit event for observability.
    fn emit_audit_event(&self, event: WalCompactionAuditEvent) -> AndromedaResult<()>;
}
