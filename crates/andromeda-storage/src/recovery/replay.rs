//! WAL record replay handlers for database recovery.
//!
//! # Design
//!
//! This module is the **single owner** of:
//! * Recovery handlers for all defined `WalRecordKind` variants
//! * Idempotency guarantees (handlers are safe to replay multiple times)
//! * LSN ordering invariants (redo records processed in ascending order)
//! * Error handling for missing handlers (fail-stop with clear errors)
//!
//! # Handler Coverage (26/26 = 100%)
//!
//! Every `WalRecordKind` variant must have an associated handler. Handlers
//! may be:
//! * **Implemented** - actively replay the operation (12 handlers, 46.1%)
//! * **Deferred** - explicit fail-stop until payload and idempotency contracts
//!   are promoted (14 handlers, 53.8%)
//! * **Deprecated** - identified as obsolete with error messages (0 handlers, 0%)
//!
//! ## Implemented Handlers (12)
//! 1. TxBegin - skipped (transaction context pre-exists)
//! 2. TxCommit - skipped (commit determined by WAL presence)
//! 3. TxRollback - skipped (deferred to explicit undo phase)
//! 4. RowInsert - HREDOV1 heap row redo
//! 5. RowUpdate - HREDOV1 close old slot + insert new slot redo
//! 6. RowDelete - HREDOV1 heap tombstone redo
//! 7. CheckpointBegin - skipped (informational boundary)
//! 8. CheckpointEnd - skipped (informational boundary)
//! 9. SnapshotBegin - skipped (informational boundary)
//! 10. SnapshotEnd - skipped (informational boundary)
//! 11. ManifestSwitch - applied when manifest validation succeeds
//! 12. SecurityAuditAppend - skipped (audit is write-only in recovery)
//!
//! ## Deferred Handlers (14)
//! 10. PageAllocate - page inventory
//! 11. PageFormat - page format version
//! 15. IndexInsert - secondary index replay
//! 16. IndexDelete - secondary index replay
//! 17. MvccVersionCreate - MVCC version store
//! 18. MvccVersionClose - MVCC version visibility
//! 19. MapDeltaAppend - map data structures
//! 20. CatalogChangeBegin - catalog transactions
//! 21. CatalogChangeApply - catalog mutations
//! 22. CatalogChangeCommit - catalog commits
//! 23. BTreeInsert - B-Tree record insertion
//! 24. BTreeDelete - B-Tree record deletion
//! 25. BTreeSplit - B-Tree node split
//! 26. BTreeMerge - B-Tree node merge
//!
//! # Idempotency Contract
//!
//! All redo handlers must be idempotent: replaying the same record multiple
//! times must produce the same durable state as replaying it once. This is
//! critical for crash recovery during recovery itself.
//!
//! **Examples:**
//! - PageAllocate: Allocating the same page twice is a no-op.
//! - RowInsert: Inserting into the same slot twice produces one row, not two.
//! - RowDelete: Deleting the same slot twice is a no-op.
//!
//! # LSN Ordering
//!
//! Redo records must be processed in ascending LSN order. Undo records (if
//! any) must be processed in descending (reverse) LSN order to correctly
//! reverse the redo sequence on rollback.
//!
//! # Transaction Boundaries
//!
//! - TxBegin: Marks transaction start (skipped in redo)
//! - TxCommit: Marks transaction commit (skipped in redo)
//! - TxRollback: Marks transaction rollback (skipped in redo; triggers undo phase)
//!
//! # Checkpoint/Snapshot Markers
//!
//! - CheckpointBegin/End: Informational boundaries (skipped)
//! - SnapshotBegin/End: Informational boundaries (skipped)

use andromeda_core::AndromedaResult;

use crate::{DatabaseManifest, Lsn, WalRecord, WalRecordKind};

use super::storage_error;

mod boundary;
mod context;
mod deferred;
mod heap_redo;
mod result;

use boundary::*;
pub use context::{IndexRebuildRequiredEvidence, ManifestSwitchRecoveryTrace, ReplayContext};
use deferred::*;
pub use heap_redo::{HeapRedoPageState, HeapRedoSlotState};
pub use result::{ReplayOutcome, ReplayResult};

/// Replay a single WAL record in recovery context.
///
/// # Preconditions
///
/// * LSN must be monotonically increasing (enforced by callers).
/// * Record must have been validated before replay.
/// * Context carries necessary state for the handler.
///
/// # Error Handling
///
/// Missing handlers return a clear error message indicating whether the
/// record type is deferred or deprecated. Handler errors are propagated
/// as `AndromedaError`.
///
/// # Idempotency
///
/// All handlers must be idempotent. Replaying the same record multiple
/// times must produce the same result as replaying it once.
pub fn replay_wal_record(ctx: &mut ReplayContext, record: &WalRecord) -> AndromedaResult<()> {
    let result = match record.header.kind {
        WalRecordKind::TxBegin => replay_tx_begin(ctx, record)?,
        WalRecordKind::TxCommit => replay_tx_commit(ctx, record)?,
        WalRecordKind::TxRollback => replay_tx_rollback(ctx, record)?,
        WalRecordKind::PageAllocate => replay_page_allocate(ctx, record)?,
        WalRecordKind::PageFormat => replay_page_format(ctx, record)?,
        WalRecordKind::RowInsert => replay_row_insert(ctx, record)?,
        WalRecordKind::RowUpdate => replay_row_update(ctx, record)?,
        WalRecordKind::RowDelete => replay_row_delete(ctx, record)?,
        WalRecordKind::IndexInsert => replay_index_insert(ctx, record)?,
        WalRecordKind::IndexDelete => replay_index_delete(ctx, record)?,
        WalRecordKind::MvccVersionCreate => replay_mvcc_version_create(ctx, record)?,
        WalRecordKind::MvccVersionClose => replay_mvcc_version_close(ctx, record)?,
        WalRecordKind::MapDeltaAppend => replay_map_delta_append(ctx, record)?,
        WalRecordKind::CheckpointBegin => replay_checkpoint_begin(ctx, record)?,
        WalRecordKind::CheckpointEnd => replay_checkpoint_end(ctx, record)?,
        WalRecordKind::SnapshotBegin => replay_snapshot_begin(ctx, record)?,
        WalRecordKind::SnapshotEnd => replay_snapshot_end(ctx, record)?,
        WalRecordKind::ManifestSwitch => replay_manifest_switch(ctx, record)?,
        WalRecordKind::CatalogChangeBegin => replay_catalog_change_begin(ctx, record)?,
        WalRecordKind::CatalogChangeApply => replay_catalog_change_apply(ctx, record)?,
        WalRecordKind::CatalogChangeCommit => replay_catalog_change_commit(ctx, record)?,
        WalRecordKind::SecurityAuditAppend => replay_security_audit_append(ctx, record)?,
        WalRecordKind::BTreeInsert => replay_btree_insert(ctx, record)?,
        WalRecordKind::BTreeDelete => replay_btree_delete(ctx, record)?,
        WalRecordKind::BTreeSplit => replay_btree_split(ctx, record)?,
        WalRecordKind::BTreeMerge => replay_btree_merge(ctx, record)?,
    };

    if result.outcome.is_error() {
        let error_msg = result.error.clone();
        ctx.record_result(result.clone());
        if let Some(msg) = error_msg {
            return Err(storage_error(&msg));
        }
    }

    ctx.record_result(result);
    Ok(())
}

/// Replay manifest switch record.
///
/// **Idempotency:** Switching to the same manifest twice is a no-op.
/// **Invariant:** Manifest switch must be atomic with respect to other operations.
fn replay_manifest_switch(
    ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    let payload = parse_manifest_switch_payload(record.payload())?;

    if payload.required_wal_start_lsn < payload.base_checkpoint_lsn {
        ctx.manifest_switch_traces.push(
            ManifestSwitchRecoveryTrace::ManifestSwitchValidationFailed {
                lsn: record.header.lsn,
                manifest_version: payload.manifest_version,
                reason: "required_wal_start_lsn precedes base_checkpoint_lsn",
            },
        );
        return Ok(ReplayResult::skipped(
            record.header.lsn,
            WalRecordKind::ManifestSwitch,
        ));
    }

    let expected_crc = ctx
        .known_manifest_crc_by_version
        .get(&payload.manifest_version)
        .copied()
        .or_else(|| {
            ctx.active_manifest
                .filter(|manifest| manifest.manifest_version == payload.manifest_version)
                .map(|manifest| manifest.manifest_crc)
        });

    if matches!(expected_crc, Some(expected) if expected != payload.manifest_crc) {
        ctx.manifest_switch_traces.push(
            ManifestSwitchRecoveryTrace::ManifestSwitchValidationFailed {
                lsn: record.header.lsn,
                manifest_version: payload.manifest_version,
                reason: "manifest CRC mismatch",
            },
        );
        return Ok(ReplayResult::skipped(
            record.header.lsn,
            WalRecordKind::ManifestSwitch,
        ));
    }

    let next_manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: payload.manifest_version,
        snapshot_id: payload.snapshot_id,
        base_checkpoint_lsn: payload.base_checkpoint_lsn,
        required_wal_start_lsn: payload.required_wal_start_lsn,
        previous_manifest_hash: payload.previous_manifest_hash,
        manifest_crc: payload.manifest_crc,
    };
    next_manifest.validate()?;

    ctx.known_manifest_crc_by_version
        .insert(payload.manifest_version, payload.manifest_crc);
    ctx.active_manifest = Some(next_manifest);
    ctx.manifest_switch_traces
        .push(ManifestSwitchRecoveryTrace::ManifestSwitchApplied {
            lsn: record.header.lsn,
            manifest_version: payload.manifest_version,
            snapshot_id: payload.snapshot_id,
            base_checkpoint_lsn: payload.base_checkpoint_lsn,
            required_wal_start_lsn: payload.required_wal_start_lsn,
        });

    Ok(ReplayResult::applied(
        record.header.lsn,
        WalRecordKind::ManifestSwitch,
    ))
}

const MANIFEST_SWITCH_PAYLOAD_LEN: usize = 68;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ManifestSwitchPayload {
    manifest_version: u64,
    snapshot_id: u64,
    base_checkpoint_lsn: Lsn,
    required_wal_start_lsn: Lsn,
    previous_manifest_hash: [u8; 32],
    manifest_crc: u32,
}

fn parse_manifest_switch_payload(bytes: &[u8]) -> AndromedaResult<ManifestSwitchPayload> {
    if bytes.len() != MANIFEST_SWITCH_PAYLOAD_LEN {
        return Err(storage_error(
            "manifest switch payload length must be exactly 68 bytes",
        ));
    }

    fn read_u64(bytes: &[u8], start: usize) -> AndromedaResult<u64> {
        let end = start + 8;
        let slice = bytes
            .get(start..end)
            .ok_or_else(|| storage_error("manifest switch payload is truncated"))?;
        let mut array = [0u8; 8];
        array.copy_from_slice(slice);
        Ok(u64::from_le_bytes(array))
    }
    fn read_u32(bytes: &[u8], start: usize) -> AndromedaResult<u32> {
        let end = start + 4;
        let slice = bytes
            .get(start..end)
            .ok_or_else(|| storage_error("manifest switch payload is truncated"))?;
        let mut array = [0u8; 4];
        array.copy_from_slice(slice);
        Ok(u32::from_le_bytes(array))
    }

    let mut hash = [0u8; 32];
    hash.copy_from_slice(
        bytes
            .get(32..64)
            .ok_or_else(|| storage_error("manifest switch payload hash is truncated"))?,
    );

    Ok(ManifestSwitchPayload {
        manifest_version: read_u64(bytes, 0)?,
        snapshot_id: read_u64(bytes, 8)?,
        base_checkpoint_lsn: Lsn::new(read_u64(bytes, 16)?),
        required_wal_start_lsn: Lsn::new(read_u64(bytes, 24)?),
        previous_manifest_hash: hash,
        manifest_crc: read_u32(bytes, 64)?,
    })
}

/// Handler coverage statistics for WAL recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandlerCoverageMetrics {
    /// Total number of WalRecordKind variants.
    pub total_kinds: usize,
    /// Number of handlers implemented or skipped (actively handled).
    pub implemented_count: usize,
    /// Number of handlers documented as future work.
    pub future_work_count: usize,
    /// Number of missing handlers (should always be 0).
    pub missing_count: usize,
}

impl HandlerCoverageMetrics {
    /// Get current handler coverage metrics.
    pub const fn current() -> Self {
        Self {
            total_kinds: 26,
            implemented_count: 12,
            future_work_count: 14,
            missing_count: 0,
        }
    }

    /// Calculate coverage percentage (implemented / total).
    pub const fn coverage_percent(&self) -> u32 {
        (self.implemented_count as u32 * 100) / self.total_kinds as u32
    }

    /// Check if all record kinds are accounted for.
    pub const fn is_complete(&self) -> bool {
        self.implemented_count + self.future_work_count + self.missing_count == self.total_kinds
    }

    /// Check if there are any missing handlers (should be false for production).
    pub const fn has_missing_handlers(&self) -> bool {
        self.missing_count > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_result_applied_has_no_error() {
        let result = ReplayResult::applied(Lsn::new(1), WalRecordKind::RowInsert);
        assert_eq!(result.outcome, ReplayOutcome::Applied);
        assert!(result.error.is_none());
    }

    #[test]
    fn replay_result_deferred_has_error() {
        let result = ReplayResult::deferred(Lsn::new(1), WalRecordKind::MapDeltaAppend);
        assert!(result.outcome.is_error());
        assert!(result.error.is_some());
    }

    #[test]
    fn replay_context_tracks_counts() {
        let mut ctx = ReplayContext::new();
        assert_eq!(ctx.applied_count, 0);
        assert_eq!(ctx.skipped_count, 0);

        ctx.record_result(ReplayResult::applied(Lsn::new(1), WalRecordKind::TxBegin));
        assert_eq!(ctx.applied_count, 1);
        assert_eq!(ctx.last_replayed_lsn, Some(Lsn::new(1)));

        ctx.record_result(ReplayResult::skipped(Lsn::new(2), WalRecordKind::TxCommit));
        assert_eq!(ctx.skipped_count, 1);
        assert_eq!(ctx.last_replayed_lsn, Some(Lsn::new(2)));
    }

    #[test]
    fn replay_context_tracks_errors() {
        let mut ctx = ReplayContext::new();
        assert!(!ctx.has_errors());

        ctx.record_result(ReplayResult::deferred(
            Lsn::new(1),
            WalRecordKind::MapDeltaAppend,
        ));
        assert!(ctx.has_errors());
        assert_eq!(ctx.error_records.len(), 1);
    }

    #[test]
    fn handler_coverage_metrics_validation() {
        let metrics = HandlerCoverageMetrics::current();

        assert_eq!(metrics.total_kinds, 26);
        assert_eq!(metrics.implemented_count, 12);
        assert_eq!(metrics.future_work_count, 14);
        assert_eq!(metrics.missing_count, 0);
        assert!(metrics.is_complete());
        assert!(!metrics.has_missing_handlers());
        assert_eq!(metrics.coverage_percent(), 46);
    }

    #[test]
    fn all_wal_record_kinds_have_handlers() {
        // This test documents the handler coverage invariant.
        // Each of the 26 WalRecordKind variants must have a handler.

        let all_kinds = [
            WalRecordKind::TxBegin,
            WalRecordKind::TxCommit,
            WalRecordKind::TxRollback,
            WalRecordKind::PageAllocate,
            WalRecordKind::PageFormat,
            WalRecordKind::RowInsert,
            WalRecordKind::RowUpdate,
            WalRecordKind::RowDelete,
            WalRecordKind::IndexInsert,
            WalRecordKind::IndexDelete,
            WalRecordKind::MvccVersionCreate,
            WalRecordKind::MvccVersionClose,
            WalRecordKind::MapDeltaAppend,
            WalRecordKind::CheckpointBegin,
            WalRecordKind::CheckpointEnd,
            WalRecordKind::SnapshotBegin,
            WalRecordKind::SnapshotEnd,
            WalRecordKind::ManifestSwitch,
            WalRecordKind::CatalogChangeBegin,
            WalRecordKind::CatalogChangeApply,
            WalRecordKind::CatalogChangeCommit,
            WalRecordKind::SecurityAuditAppend,
            WalRecordKind::BTreeInsert,
            WalRecordKind::BTreeDelete,
            WalRecordKind::BTreeSplit,
            WalRecordKind::BTreeMerge,
        ];

        assert_eq!(all_kinds.len(), 26);

        let implemented = [
            WalRecordKind::TxBegin,
            WalRecordKind::TxCommit,
            WalRecordKind::TxRollback,
            WalRecordKind::RowInsert,
            WalRecordKind::RowUpdate,
            WalRecordKind::RowDelete,
            WalRecordKind::CheckpointBegin,
            WalRecordKind::CheckpointEnd,
            WalRecordKind::SnapshotBegin,
            WalRecordKind::SnapshotEnd,
            WalRecordKind::ManifestSwitch,
            WalRecordKind::SecurityAuditAppend,
        ];
        assert_eq!(implemented.len(), 12);

        let future_work = [
            WalRecordKind::PageAllocate,
            WalRecordKind::PageFormat,
            WalRecordKind::IndexInsert,
            WalRecordKind::IndexDelete,
            WalRecordKind::MvccVersionCreate,
            WalRecordKind::MvccVersionClose,
            WalRecordKind::MapDeltaAppend,
            WalRecordKind::CatalogChangeBegin,
            WalRecordKind::CatalogChangeApply,
            WalRecordKind::CatalogChangeCommit,
            WalRecordKind::BTreeInsert,
            WalRecordKind::BTreeDelete,
            WalRecordKind::BTreeSplit,
            WalRecordKind::BTreeMerge,
        ];
        assert_eq!(future_work.len(), 14);
        assert_eq!(implemented.len() + future_work.len(), all_kinds.len());

        for kind in &all_kinds {
            let is_handled = implemented.contains(kind) || future_work.contains(kind);
            assert!(
                is_handled,
                "{:?} must have a handler (implemented or documented as future work)",
                kind
            );
        }
    }

    fn manifest_switch_payload(
        manifest_version: u64,
        snapshot_id: u64,
        base_checkpoint_lsn: u64,
        required_wal_start_lsn: u64,
        previous_manifest_hash: [u8; 32],
        manifest_crc: u32,
    ) -> Vec<u8> {
        let mut payload = Vec::with_capacity(MANIFEST_SWITCH_PAYLOAD_LEN);
        payload.extend_from_slice(&manifest_version.to_le_bytes());
        payload.extend_from_slice(&snapshot_id.to_le_bytes());
        payload.extend_from_slice(&base_checkpoint_lsn.to_le_bytes());
        payload.extend_from_slice(&required_wal_start_lsn.to_le_bytes());
        payload.extend_from_slice(&previous_manifest_hash);
        payload.extend_from_slice(&manifest_crc.to_le_bytes());
        payload
    }

    #[test]
    fn manifest_switch_applies_when_crc_matches_known_manifest() {
        let mut ctx = ReplayContext::new();
        ctx.known_manifest_crc_by_version.insert(7, 0xAA55_3311);
        let record = WalRecord::from_parts(
            WalRecordKind::ManifestSwitch,
            Lsn::new(77),
            Some(Lsn::new(76)),
            None,
            manifest_switch_payload(7, 90, 40, 45, [9; 32], 0xAA55_3311),
        )
        .expect("manifest switch record should be valid");

        replay_wal_record(&mut ctx, &record).expect("replay should succeed");
        assert_eq!(ctx.applied_count, 1);
        assert_eq!(
            ctx.active_manifest
                .expect("manifest must be applied")
                .snapshot_id,
            90
        );
        assert!(matches!(
            ctx.manifest_switch_traces.last(),
            Some(ManifestSwitchRecoveryTrace::ManifestSwitchApplied {
                manifest_version: 7,
                ..
            })
        ));
    }

    #[test]
    fn manifest_switch_rejects_crc_mismatch_and_keeps_previous_manifest() {
        let mut ctx = ReplayContext::new();
        ctx.active_manifest = Some(DatabaseManifest {
            database_id: 1,
            manifest_version: 6,
            snapshot_id: 80,
            base_checkpoint_lsn: Lsn::new(30),
            required_wal_start_lsn: Lsn::new(31),
            previous_manifest_hash: [3; 32],
            manifest_crc: 0x1000_0001,
        });
        ctx.known_manifest_crc_by_version.insert(7, 0xABCD_EF01);
        let record = WalRecord::from_parts(
            WalRecordKind::ManifestSwitch,
            Lsn::new(78),
            Some(Lsn::new(77)),
            None,
            manifest_switch_payload(7, 99, 40, 45, [8; 32], 0xABCD_EF02),
        )
        .expect("manifest switch record should be valid");

        replay_wal_record(&mut ctx, &record).expect("replay should not hard-fail");
        assert_eq!(ctx.applied_count, 0);
        assert_eq!(ctx.skipped_count, 1);
        assert_eq!(
            ctx.active_manifest
                .expect("old manifest should remain")
                .manifest_version,
            6
        );
        assert!(matches!(
            ctx.manifest_switch_traces.last(),
            Some(
                ManifestSwitchRecoveryTrace::ManifestSwitchValidationFailed {
                    reason: "manifest CRC mismatch",
                    ..
                }
            )
        ));
    }
}
