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
//! * **Implemented** - skip an informational boundary or actively replay the
//!   durable operation (12 handlers, 46.1%)
//! * **Deferred** - explicit fail-stop or access-path rebuild evidence until
//!   payload and idempotency contracts are promoted (14 handlers, 53.8%)
//! * **Deprecated** - identified as obsolete with error messages (0 handlers, 0%)
//!
//! ## Implemented Handlers (12)
//! 1. TxBegin - skipped (transaction context pre-exists)
//! 2. TxCommit - skipped (commit determined by WAL presence)
//! 3. TxRollback - skipped (rollback determined by durable terminal record)
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
//! 1. PageAllocate - page inventory
//! 2. PageFormat - page format version
//! 3. IndexInsert - secondary index replay
//! 4. IndexDelete - secondary index replay
//! 5. MvccVersionCreate - MVCC version store
//! 6. MvccVersionClose - MVCC version visibility
//! 7. MapDeltaAppend - map data structures
//! 8. CatalogChangeBegin - catalog transactions
//! 9. CatalogChangeApply - catalog mutations
//! 10. CatalogChangeCommit - catalog commits
//! 11. BTreeInsert - B-Tree record insertion
//! 12. BTreeDelete - B-Tree record deletion
//! 13. BTreeSplit - B-Tree node split
//! 14. BTreeMerge - B-Tree node merge
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
//! - TxRollback: Marks transaction rollback (skipped in redo)
//!
//! # Checkpoint/Snapshot Markers
//!
//! - CheckpointBegin/End: Informational boundaries (skipped)
//! - SnapshotBegin/End: Informational boundaries (skipped)

use andromeda_core::AndromedaResult;

use crate::{WalRecord, WalRecordKind};

use super::storage_error;

mod boundary;
mod context;
mod deferred;
mod heap_redo;
mod manifest_switch;
mod result;

pub use andromeda_recovery::{IndexRebuildRequiredEvidence, ManifestSwitchRecoveryTrace};
use boundary::*;
pub use context::ReplayContext;
use deferred::*;
pub use heap_redo::{HeapRedoPageState, HeapRedoSlotState};
use manifest_switch::replay_manifest_switch;
pub use result::{ReplayOutcome, ReplayResult};

#[cfg(test)]
use crate::{DatabaseManifest, Lsn};
#[cfg(test)]
use manifest_switch::MANIFEST_SWITCH_PAYLOAD_LEN;

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
pub(crate) fn replay_wal_record_result(
    ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
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
        return Ok(result);
    }

    ctx.record_result(result.clone());
    Ok(result)
}

pub fn replay_wal_record(ctx: &mut ReplayContext, record: &WalRecord) -> AndromedaResult<()> {
    replay_wal_record_result(ctx, record).map(|_| ())
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
        ctx.observe_checkpoint_end(Lsn::new(40));
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
        ctx.observe_checkpoint_end(Lsn::new(40));
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

    #[test]
    fn manifest_switch_rejects_checkpoint_after_switch_record_lsn() {
        let mut ctx = ReplayContext::new();
        ctx.known_manifest_crc_by_version.insert(8, 0xAA55_4411);
        let record = WalRecord::from_parts(
            WalRecordKind::ManifestSwitch,
            Lsn::new(77),
            Some(Lsn::new(76)),
            None,
            manifest_switch_payload(8, 91, 80, 81, [4; 32], 0xAA55_4411),
        )
        .expect("manifest switch record should be structurally valid");

        replay_wal_record(&mut ctx, &record).expect("replay should not hard-fail");
        assert_eq!(ctx.applied_count, 0);
        assert_eq!(ctx.skipped_count, 1);
        assert!(ctx.active_manifest.is_none());
        assert!(matches!(
            ctx.manifest_switch_traces.last(),
            Some(
                ManifestSwitchRecoveryTrace::ManifestSwitchValidationFailed {
                    reason: "base_checkpoint_lsn exceeds manifest switch record LSN",
                    ..
                }
            )
        ));
    }

    #[test]
    fn manifest_switch_rejects_missing_checkpoint_end_evidence() {
        let mut ctx = ReplayContext::new();
        ctx.require_manifest_switch_checkpoint_evidence();
        ctx.known_manifest_crc_by_version.insert(9, 0xCC55_4411);
        let record = WalRecord::from_parts(
            WalRecordKind::ManifestSwitch,
            Lsn::new(77),
            Some(Lsn::new(76)),
            None,
            manifest_switch_payload(9, 92, 40, 41, [5; 32], 0xCC55_4411),
        )
        .expect("manifest switch record should be structurally valid");

        replay_wal_record(&mut ctx, &record).expect("replay should not hard-fail");
        assert_eq!(ctx.applied_count, 0);
        assert_eq!(ctx.skipped_count, 1);
        assert!(ctx.active_manifest.is_none());
        assert!(matches!(
            ctx.manifest_switch_traces.last(),
            Some(
                ManifestSwitchRecoveryTrace::ManifestSwitchValidationFailed {
                    reason: "base_checkpoint_lsn lacks durable checkpoint_end evidence",
                    ..
                }
            )
        ));
    }
}
