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
//! # Handler Coverage
//!
//! Every `WalRecordKind` variant must have an associated handler. Handlers
//! may be:
//! * **Implemented** — actively replay the operation
//! * **Future work** — documented with clear error messages
//! * **Deprecated** — identified as obsolete with error messages
//!
//! # Idempotency Contract
//!
//! All redo handlers must be idempotent: replaying the same record multiple
//! times must produce the same durable state as replaying it once. This is
//! critical for crash recovery during recovery itself.
//!
//! # LSN Ordering
//!
//! Redo records must be processed in ascending LSN order. Undo records (if
//! any) must be processed in descending (reverse) LSN order to correctly
//! reverse the redo sequence on rollback.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{Lsn, WalRecord, WalRecordKind};

use super::storage_error;

/// Handler result for a single WAL record replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayOutcome {
    /// Record was successfully applied to the database state.
    Applied,
    /// Record was skipped (not applicable to this recovery context).
    Skipped,
    /// Record is not yet implemented; database may be corrupted if
    /// records of this type are present.
    NotYetImplemented,
    /// Record is deprecated and should not appear in new WAL files.
    Deprecated,
}

impl ReplayOutcome {
    pub const fn is_applied(self) -> bool {
        matches!(self, Self::Applied)
    }

    pub const fn is_error(self) -> bool {
        matches!(self, Self::NotYetImplemented | Self::Deprecated)
    }
}

/// Result of replaying a single WAL record with metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayResult {
    pub lsn: Lsn,
    pub kind: WalRecordKind,
    pub outcome: ReplayOutcome,
    pub error: Option<String>,
}

impl ReplayResult {
    pub fn applied(lsn: Lsn, kind: WalRecordKind) -> Self {
        Self {
            lsn,
            kind,
            outcome: ReplayOutcome::Applied,
            error: None,
        }
    }

    pub fn skipped(lsn: Lsn, kind: WalRecordKind) -> Self {
        Self {
            lsn,
            kind,
            outcome: ReplayOutcome::Skipped,
            error: None,
        }
    }

    pub fn not_yet_implemented(lsn: Lsn, kind: WalRecordKind) -> Self {
        Self {
            lsn,
            kind,
            outcome: ReplayOutcome::NotYetImplemented,
            error: Some(format!(
                "{:?} recovery not yet implemented; database may be corrupted if records of this type are present.",
                kind
            )),
        }
    }

    pub fn deprecated(lsn: Lsn, kind: WalRecordKind) -> Self {
        Self {
            lsn,
            kind,
            outcome: ReplayOutcome::Deprecated,
            error: Some(format!(
                "{:?} is deprecated and should not appear in new WAL files.",
                kind
            )),
        }
    }

    pub fn error(lsn: Lsn, kind: WalRecordKind, error_msg: impl Into<String>) -> Self {
        Self {
            lsn,
            kind,
            outcome: ReplayOutcome::NotYetImplemented,
            error: Some(error_msg.into()),
        }
    }
}

/// Replay context for database recovery.
///
/// This context is passed to all replay handlers and carries the state
/// needed to apply WAL records to the database.
pub struct ReplayContext {
    // TODO: Add fields for heap pages, index structures, catalog, MVCC version store, etc.
    //       as recovery handlers are implemented.
    /// Highest LSN replayed so far in this recovery session.
    pub last_replayed_lsn: Option<Lsn>,

    /// Count of successfully applied records.
    pub applied_count: usize,

    /// Count of skipped records.
    pub skipped_count: usize,

    /// Records that failed to apply.
    pub error_records: Vec<ReplayResult>,
}

impl ReplayContext {
    pub fn new() -> Self {
        Self {
            last_replayed_lsn: None,
            applied_count: 0,
            skipped_count: 0,
            error_records: Vec::new(),
        }
    }

    pub fn record_result(&mut self, result: ReplayResult) {
        self.last_replayed_lsn = Some(result.lsn);

        match result.outcome {
            ReplayOutcome::Applied => self.applied_count += 1,
            ReplayOutcome::Skipped => self.skipped_count += 1,
            ReplayOutcome::NotYetImplemented | ReplayOutcome::Deprecated => {
                self.error_records.push(result);
            }
        }
    }

    pub fn has_errors(&self) -> bool {
        !self.error_records.is_empty()
    }
}

impl Default for ReplayContext {
    fn default() -> Self {
        Self::new()
    }
}

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
/// record type is future work or deprecated. Handler errors are propagated
/// as `AndromedaError`.
///
/// # Idempotency
///
/// All handlers must be idempotent. Replaying the same record multiple
/// times must produce the same result as replaying it once.
pub fn replay_wal_record(ctx: &mut ReplayContext, record: &WalRecord) -> AndromedaResult<()> {
    let result = match record.header.kind {
        // === Transaction Boundary Records ===
        WalRecordKind::TxBegin => {
            // Create transaction context in recovery state.
            // This is a no-op in recovery; the transaction already exists.
            ReplayResult::skipped(record.header.lsn, WalRecordKind::TxBegin)
        }

        WalRecordKind::TxCommit => {
            // Add transaction to commit set and skip undo.
            // This is a no-op in recovery; commit is determined by presence in WAL.
            ReplayResult::skipped(record.header.lsn, WalRecordKind::TxCommit)
        }

        WalRecordKind::TxRollback => {
            // Add transaction to rollback set and perform undo.
            // Rollback handling is deferred to explicit undo phase.
            ReplayResult::skipped(record.header.lsn, WalRecordKind::TxRollback)
        }

        // === Page Management Records ===
        WalRecordKind::PageAllocate => {
            // Mark page as allocated in the page inventory.
            replay_page_allocate(ctx, record)?
        }

        WalRecordKind::PageFormat => {
            // Validate and apply page format version.
            replay_page_format(ctx, record)?
        }

        // === Row-Level Mutation Records ===
        WalRecordKind::RowInsert => {
            // Replay inserted row into heap page.
            replay_row_insert(ctx, record)?
        }

        WalRecordKind::RowUpdate => {
            // Replay updated row in heap page.
            replay_row_update(ctx, record)?
        }

        WalRecordKind::RowDelete => {
            // Replay deletion marker on heap page slot.
            replay_row_delete(ctx, record)?
        }

        // === Index Mutation Records ===
        WalRecordKind::IndexInsert => {
            // Replay index entry insertion.
            replay_index_insert(ctx, record)?
        }

        WalRecordKind::IndexDelete => {
            // Replay index entry deletion.
            replay_index_delete(ctx, record)?
        }

        // === MVCC Version Management Records ===
        WalRecordKind::MvccVersionCreate => {
            // Create row version header in MVCC store.
            replay_mvcc_version_create(ctx, record)?
        }

        WalRecordKind::MvccVersionClose => {
            // Close row version on transaction commit.
            replay_mvcc_version_close(ctx, record)?
        }

        // === Map / Delta Records ===
        WalRecordKind::MapDeltaAppend => {
            // Append delta to map-based structure (future work).
            ReplayResult::not_yet_implemented(record.header.lsn, WalRecordKind::MapDeltaAppend)
        }

        // === Checkpoint Boundary Records ===
        WalRecordKind::CheckpointBegin => {
            // Mark checkpoint begin boundary.
            replay_checkpoint_begin(ctx, record)?
        }

        WalRecordKind::CheckpointEnd => {
            // Mark checkpoint end boundary.
            replay_checkpoint_end(ctx, record)?
        }

        // === Cold Snapshot Boundary Records ===
        WalRecordKind::SnapshotBegin => {
            // Mark cold snapshot begin boundary.
            replay_snapshot_begin(ctx, record)?
        }

        WalRecordKind::SnapshotEnd => {
            // Mark cold snapshot end boundary.
            replay_snapshot_end(ctx, record)?
        }

        // === Manifest and Storage Records ===
        WalRecordKind::ManifestSwitch => {
            // Apply manifest switch to storage engine.
            replay_manifest_switch(ctx, record)?
        }

        // === Catalog Change Records ===
        WalRecordKind::CatalogChangeBegin => {
            // Mark catalog change transaction begin.
            ReplayResult::not_yet_implemented(record.header.lsn, WalRecordKind::CatalogChangeBegin)
        }

        WalRecordKind::CatalogChangeApply => {
            // Apply catalog mutation (create/alter/drop).
            ReplayResult::not_yet_implemented(record.header.lsn, WalRecordKind::CatalogChangeApply)
        }

        WalRecordKind::CatalogChangeCommit => {
            // Commit catalog changes.
            ReplayResult::not_yet_implemented(record.header.lsn, WalRecordKind::CatalogChangeCommit)
        }

        // === Security and Audit Records ===
        WalRecordKind::SecurityAuditAppend => {
            // Append audit entry (may be write-only in recovery).
            ReplayResult::skipped(record.header.lsn, WalRecordKind::SecurityAuditAppend)
        }
    };

    // Check for unimplemented handlers and fail-stop with clear error.
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

// === Handler Implementations ===

/// Replay page allocation record.
///
/// **Idempotency:** Allocating the same page twice is a no-op.
fn replay_page_allocate(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TODO: Mark page as allocated in page inventory
    //       - Extract page ID from record payload
    //       - Mark corresponding bit in page bitmap
    //       - Update page header to reflect allocation
    Ok(ReplayResult::not_yet_implemented(
        _record.header.lsn,
        WalRecordKind::PageAllocate,
    ))
}

/// Replay page format initialization record.
///
/// **Idempotency:** Formatting the same page twice is a no-op.
/// **Invariant:** PageFormat must precede any mutation on the page.
fn replay_page_format(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TODO: Initialize page format
    //       - Extract page ID and format version from payload
    //       - Initialize page header with format version
    //       - Validate format compatibility
    Ok(ReplayResult::not_yet_implemented(
        _record.header.lsn,
        WalRecordKind::PageFormat,
    ))
}

/// Replay row insert record.
///
/// **Idempotency:** Inserting into the same slot twice is a no-op if the
/// slot is already visible and contains the same row data.
/// **Undo:** RowInsert undo is implemented as RowDelete (mark slot as deleted).
fn replay_row_insert(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TODO: Replay inserted row into heap page
    //       - Extract page ID, slot, and row data from payload
    //       - Locate heap page in page cache
    //       - Allocate or reuse slot
    //       - Write row data to slot
    //       - Mark slot as visible (transaction committed)
    Ok(ReplayResult::not_yet_implemented(
        _record.header.lsn,
        WalRecordKind::RowInsert,
    ))
}

/// Replay row update record.
///
/// **Idempotency:** Updating the same row twice is a no-op if the row is
/// already at the target version.
/// **Undo:** RowUpdate undo is implemented as restoring the previous version.
fn replay_row_update(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TODO: Replay updated row in heap page
    //       - Extract page ID, slot, and new row data from payload
    //       - Locate heap page in page cache
    //       - Overwrite slot data with updated row
    //       - Mark slot as updated (transaction committed)
    Ok(ReplayResult::not_yet_implemented(
        _record.header.lsn,
        WalRecordKind::RowUpdate,
    ))
}

/// Replay row deletion record.
///
/// **Idempotency:** Deleting the same slot twice is a no-op.
/// **Undo:** RowDelete undo is implemented by clearing the deletion marker.
fn replay_row_delete(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TODO: Replay deletion marker on heap page slot
    //       - Extract page ID and slot from payload
    //       - Locate heap page in page cache
    //       - Mark slot as deleted (transaction committed)
    //       - Note: Row data may be preserved for MVCC or can be zeroed
    Ok(ReplayResult::not_yet_implemented(
        _record.header.lsn,
        WalRecordKind::RowDelete,
    ))
}

/// Replay index entry insertion record.
///
/// **Idempotency:** Inserting the same index entry twice is a no-op.
/// **Undo:** IndexInsert undo is implemented as IndexDelete.
/// **Note:** Index insertion may be deferred to post-recovery index rebuild.
fn replay_index_insert(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TODO: Replay index entry insertion
    //       - Extract index ID and key+value from payload
    //       - Locate index structure in memory
    //       - Insert entry into index B-tree or hash structure
    //       - Update index metadata
    Ok(ReplayResult::not_yet_implemented(
        _record.header.lsn,
        WalRecordKind::IndexInsert,
    ))
}

/// Replay index entry deletion record.
///
/// **Idempotency:** Deleting the same index entry twice is a no-op.
/// **Undo:** IndexDelete undo is implemented as IndexInsert.
fn replay_index_delete(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TODO: Replay index entry deletion
    //       - Extract index ID and key from payload
    //       - Locate index structure in memory
    //       - Delete entry from index B-tree or hash structure
    //       - Update index metadata
    Ok(ReplayResult::not_yet_implemented(
        _record.header.lsn,
        WalRecordKind::IndexDelete,
    ))
}

/// Replay MVCC version creation record.
///
/// **Idempotency:** Creating the same version twice is a no-op.
/// **Invariant:** MvccVersionCreate must precede corresponding row mutations.
fn replay_mvcc_version_create(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TODO: Create row version header in MVCC store
    //       - Extract row ID and transaction ID from payload
    //       - Allocate MVCC version header
    //       - Link to transaction context
    //       - Set version visibility based on transaction state
    Ok(ReplayResult::not_yet_implemented(
        _record.header.lsn,
        WalRecordKind::MvccVersionCreate,
    ))
}

/// Replay MVCC version close record.
///
/// **Idempotency:** Closing the same version twice is a no-op.
/// **Invariant:** MvccVersionClose records when a version becomes immutable.
fn replay_mvcc_version_close(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TODO: Close row version on transaction commit
    //       - Extract row ID and transaction ID from payload
    //       - Locate MVCC version header
    //       - Mark version as committed (visible to future transactions)
    //       - Update visibility snapshot
    Ok(ReplayResult::not_yet_implemented(
        _record.header.lsn,
        WalRecordKind::MvccVersionClose,
    ))
}

/// Replay checkpoint begin marker.
///
/// **Idempotency:** Begin markers are idempotent; replay is a no-op.
fn replay_checkpoint_begin(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TODO: Record checkpoint begin boundary (informational)
    //       - Log checkpoint start LSN
    //       - Mark recovery checkpoint boundary
    Ok(ReplayResult::skipped(
        _record.header.lsn,
        WalRecordKind::CheckpointBegin,
    ))
}

/// Replay checkpoint end marker.
///
/// **Idempotency:** End markers are idempotent; replay is a no-op.
fn replay_checkpoint_end(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TODO: Record checkpoint end boundary (informational)
    //       - Log checkpoint end LSN
    //       - Validate checkpoint interval
    Ok(ReplayResult::skipped(
        _record.header.lsn,
        WalRecordKind::CheckpointEnd,
    ))
}

/// Replay cold snapshot begin marker.
///
/// **Idempotency:** Begin markers are idempotent; replay is a no-op.
fn replay_snapshot_begin(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TODO: Record cold snapshot begin boundary (informational)
    //       - Log snapshot start LSN
    Ok(ReplayResult::skipped(
        _record.header.lsn,
        WalRecordKind::SnapshotBegin,
    ))
}

/// Replay cold snapshot end marker.
///
/// **Idempotency:** End markers are idempotent; replay is a no-op.
fn replay_snapshot_end(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TODO: Record cold snapshot end boundary (informational)
    //       - Log snapshot end LSN
    Ok(ReplayResult::skipped(
        _record.header.lsn,
        WalRecordKind::SnapshotEnd,
    ))
}

/// Replay manifest switch record.
///
/// **Idempotency:** Switching to the same manifest twice is a no-op.
/// **Invariant:** Manifest switch must be atomic with respect to other operations.
fn replay_manifest_switch(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TODO: Apply manifest switch to storage engine
    //       - Extract new manifest from payload
    //       - Validate manifest consistency
    //       - Switch buffer pool metadata
    //       - Update extent allocator state
    Ok(ReplayResult::not_yet_implemented(
        _record.header.lsn,
        WalRecordKind::ManifestSwitch,
    ))
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
    fn replay_result_not_yet_implemented_has_error() {
        let result = ReplayResult::not_yet_implemented(Lsn::new(1), WalRecordKind::MapDeltaAppend);
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

        ctx.record_result(ReplayResult::not_yet_implemented(
            Lsn::new(1),
            WalRecordKind::MapDeltaAppend,
        ));
        assert!(ctx.has_errors());
        assert_eq!(ctx.error_records.len(), 1);
    }
}
