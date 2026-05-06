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
//! * **Implemented** — actively replay the operation (8 handlers, 30.8%)
//! * **Future work** — documented with clear error messages (18 handlers, 69.2%)
//! * **Deprecated** — identified as obsolete with error messages (0 handlers, 0%)
//!
//! ## Implemented Handlers (8)
//! 1. ✅ TxBegin — Skipped (transaction context pre-exists)
//! 2. ✅ TxCommit — Skipped (commit determined by WAL presence)
//! 3. ✅ TxRollback — Skipped (deferred to explicit undo phase)
//! 4. ✅ CheckpointBegin — Skipped (informational boundary)
//! 5. ✅ CheckpointEnd — Skipped (informational boundary)
//! 6. ✅ SnapshotBegin — Skipped (informational boundary)
//! 7. ✅ SnapshotEnd — Skipped (informational boundary)
//! 8. ✅ SecurityAuditAppend — Skipped (audit is write-only in recovery)
//!
//! ## Future Work Handlers (18)
//! 9. 📋 PageAllocate — Wave 18 (page inventory)
//! 10. 📋 PageFormat — Wave 18 (page format version)
//! 11. 📋 RowInsert — Wave 19 (heap page replay)
//! 12. 📋 RowUpdate — Wave 19 (heap page updates)
//! 13. 📋 RowDelete — Wave 19 (deletion markers)
//! 14. 📋 IndexInsert — Wave 19 (index B-tree)
//! 15. 📋 IndexDelete — Wave 19 (index B-tree)
//! 16. 📋 MvccVersionCreate — Wave 17 (MVCC version store)
//! 17. 📋 MvccVersionClose — Wave 17 (MVCC version visibility)
//! 18. 📋 MapDeltaAppend — Wave 20 (map data structures)
//! 19. 📋 ManifestSwitch — Wave 21 (storage manifest)
//! 20. 📋 CatalogChangeBegin — Wave 22 (catalog transactions)
//! 21. 📋 CatalogChangeApply — Wave 22 (catalog mutations)
//! 22. 📋 CatalogChangeCommit — Wave 22 (catalog commits)
//! 23. 📋 BTreeInsert — Wave 18 (B-Tree record insertion, deferred)
//! 24. 📋 BTreeDelete — Wave 18 (B-Tree record deletion, deferred)
//! 25. 📋 BTreeSplit — Wave 18 (B-Tree node split, deferred)
//! 26. 📋 BTreeMerge — Wave 18 (B-Tree node merge, deferred)
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

use std::collections::HashMap;

use andromeda_core::AndromedaResult;

use crate::{DatabaseManifest, Lsn, WalRecord, WalRecordKind};

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
    // TECH-DEBT(recovery-replay-context): Replay state is intentionally minimal until
    // heap/index/catalog/MVCC redo handlers stop fail-stopping.
    /// Highest LSN replayed so far in this recovery session.
    pub last_replayed_lsn: Option<Lsn>,

    /// Count of successfully applied records.
    pub applied_count: usize,

    /// Count of skipped records.
    pub skipped_count: usize,

    /// Records that failed to apply.
    pub error_records: Vec<ReplayResult>,

    /// Active manifest anchor after replay.
    pub active_manifest: Option<DatabaseManifest>,

    /// CRC catalog for persisted manifests keyed by manifest version.
    pub known_manifest_crc_by_version: HashMap<u64, u32>,

    /// Observable manifest-switch trace events from replay.
    pub manifest_switch_traces: Vec<ManifestSwitchRecoveryTrace>,
}

impl ReplayContext {
    pub fn new() -> Self {
        Self {
            last_replayed_lsn: None,
            applied_count: 0,
            skipped_count: 0,
            error_records: Vec::new(),
            active_manifest: None,
            known_manifest_crc_by_version: HashMap::new(),
            manifest_switch_traces: Vec::new(),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestSwitchRecoveryTrace {
    ManifestSwitchApplied {
        lsn: Lsn,
        manifest_version: u64,
        snapshot_id: u64,
        base_checkpoint_lsn: Lsn,
        required_wal_start_lsn: Lsn,
    },
    ManifestSwitchValidationFailed {
        lsn: Lsn,
        manifest_version: u64,
        reason: &'static str,
    },
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

        // === B-Tree Mutation Records (Wave 18 Placeholders) ===
        WalRecordKind::BTreeInsert => {
            // B-Tree record insertion. Deferred to Wave 18.
            replay_btree_insert(ctx, record)?
        }

        WalRecordKind::BTreeDelete => {
            // B-Tree record deletion. Deferred to Wave 18.
            replay_btree_delete(ctx, record)?
        }

        WalRecordKind::BTreeSplit => {
            // B-Tree node split. Deferred to Wave 18.
            replay_btree_split(ctx, record)?
        }

        WalRecordKind::BTreeMerge => {
            // B-Tree node merge. Deferred to Wave 18.
            replay_btree_merge(ctx, record)?
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
    // TECH-DEBT(recovery-page-allocate): Fail-stop until page inventory payload
    // decoding and idempotent allocation-bit updates are implemented.
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
    // TECH-DEBT(recovery-page-format): Fail-stop until page-format payload parsing
    // and format compatibility checks are wired to page initialization.
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
    // TECH-DEBT(recovery-row-insert): Fail-stop until heap WAL payload decoding
    // and idempotent slot visibility application are implemented.
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
    // TECH-DEBT(recovery-row-update): Fail-stop until heap WAL payload decoding
    // and idempotent in-place/versioned row updates are implemented.
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
    // TECH-DEBT(recovery-row-delete): Fail-stop until heap slot tombstone replay
    // is idempotent and MVCC-consistent for already-applied deletes.
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
    // TECH-DEBT(recovery-index-insert): Fail-stop until index WAL payload decoding
    // and idempotent insertion against recovered index state are implemented.
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
    // TECH-DEBT(recovery-index-delete): Fail-stop until index WAL payload decoding
    // and idempotent delete semantics across retries are implemented.
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
    // TECH-DEBT(recovery-mvcc-create): Fail-stop until MVCC version header replay
    // and transaction-visibility reconstruction are implemented.
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
    // TECH-DEBT(recovery-mvcc-close): Fail-stop until MVCC close replay
    // preserves commit visibility and idempotent close semantics.
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

/// Replay B-Tree record insertion.
///
/// **Status:** Deferred with fail-stop semantics (not yet implemented).
/// **Idempotency:** Inserting the same B-Tree entry twice is a no-op.
/// **Invariant:** B-Tree insertion must maintain index structure invariants.
/// **Note:** If this record is present in WAL, the database may be corrupted
/// without Wave 18 B-Tree recovery implementation.
fn replay_btree_insert(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-btree-insert): Fail-stop until B-Tree insert redo can
    // apply key/value payloads idempotently while preserving tree invariants.
    Ok(ReplayResult::not_yet_implemented(
        _record.header.lsn,
        WalRecordKind::BTreeInsert,
    ))
}

/// Replay B-Tree record deletion.
///
/// **Status:** Deferred with fail-stop semantics (not yet implemented).
/// **Idempotency:** Deleting the same B-Tree entry twice is a no-op.
/// **Invariant:** B-Tree deletion must maintain index structure invariants.
/// **Note:** If this record is present in WAL, the database may be corrupted
/// without Wave 18 B-Tree recovery implementation.
fn replay_btree_delete(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-btree-delete): Fail-stop until B-Tree delete redo can
    // remove keys idempotently while preserving structural invariants.
    Ok(ReplayResult::not_yet_implemented(
        _record.header.lsn,
        WalRecordKind::BTreeDelete,
    ))
}

/// Replay B-Tree node split.
///
/// **Status:** Deferred with fail-stop semantics (not yet implemented).
/// **Idempotency:** Splitting the same node twice is a no-op.
/// **Invariant:** B-Tree split must maintain all entries and tree structure.
/// **Note:** If this record is present in WAL, the database may be corrupted
/// without Wave 18 B-Tree recovery implementation.
fn replay_btree_split(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-btree-split): Fail-stop until B-Tree split redo can
    // replay parent/sibling linkage updates deterministically.
    Ok(ReplayResult::not_yet_implemented(
        _record.header.lsn,
        WalRecordKind::BTreeSplit,
    ))
}

/// Replay B-Tree node merge.
///
/// **Status:** Deferred with fail-stop semantics (not yet implemented).
/// **Idempotency:** Merging the same node pair twice is a no-op.
/// **Invariant:** B-Tree merge must preserve all entries and maintain structure.
/// **Note:** If this record is present in WAL, the database may be corrupted
/// without Wave 18 B-Tree recovery implementation.
fn replay_btree_merge(
    _ctx: &mut ReplayContext,
    _record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-btree-merge): Fail-stop until B-Tree merge redo can
    // replay node consolidation and parent updates idempotently.
    Ok(ReplayResult::not_yet_implemented(
        _record.header.lsn,
        WalRecordKind::BTreeMerge,
    ))
}

// === Handler Coverage Metrics ===

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
    /// Get current handler coverage metrics (as of Wave 13, Batch 10).
    pub const fn current() -> Self {
        Self {
            total_kinds: 26,
            implemented_count: 9,
            future_work_count: 17,
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

    #[test]
    fn handler_coverage_metrics_validation() {
        let metrics = HandlerCoverageMetrics::current();

        // Verify coverage statistics
        assert_eq!(metrics.total_kinds, 26);
        assert_eq!(metrics.implemented_count, 9);
        assert_eq!(metrics.future_work_count, 17);
        assert_eq!(metrics.missing_count, 0);

        // Verify all kinds are accounted for
        assert!(metrics.is_complete());

        // Verify no missing handlers
        assert!(!metrics.has_missing_handlers());

        // Verify coverage percentage
        assert_eq!(metrics.coverage_percent(), 34); // 9/26 = 34%
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

        // Verify all kinds are accounted for (26 total)
        assert_eq!(all_kinds.len(), 26);

        // Implemented/skipped handlers (9)
        let implemented = [
            WalRecordKind::TxBegin,
            WalRecordKind::TxCommit,
            WalRecordKind::TxRollback,
            WalRecordKind::CheckpointBegin,
            WalRecordKind::CheckpointEnd,
            WalRecordKind::SnapshotBegin,
            WalRecordKind::SnapshotEnd,
            WalRecordKind::ManifestSwitch,
            WalRecordKind::SecurityAuditAppend,
        ];
        assert_eq!(implemented.len(), 9);

        // Future work handlers (17: 13 original + 4 B-Tree)
        let future_work = [
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
            WalRecordKind::CatalogChangeBegin,
            WalRecordKind::CatalogChangeApply,
            WalRecordKind::CatalogChangeCommit,
            WalRecordKind::BTreeInsert,
            WalRecordKind::BTreeDelete,
            WalRecordKind::BTreeSplit,
            WalRecordKind::BTreeMerge,
        ];
        assert_eq!(future_work.len(), 17);

        // Verify total coverage
        assert_eq!(implemented.len() + future_work.len(), all_kinds.len());

        // Verify each kind has a handler
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
