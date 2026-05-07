//! WAL replay from a manifest-required start LSN during crash recovery.
//!
//! This module provides [`replay_wal_from_lsn`], the primary entry point that
//! drives the redo pass over durable WAL records guided by a
//! [`super::planning::ConceptualRedoPlan`].
//!
//! # Recovery Protocol (Redo Pass)
//!
//! 1. **Validate manifest** — snapshot ID, LSN anchor, storage format fingerprints.
//! 2. **Build `ConceptualRedoPlan`** — classify every durable record:
//!    `Replay`, `SkipIncompleteTransaction`, `SkipNonRedoRecord`, etc.
//! 3. **Redo pass** — apply every `Replay`-classified record in ascending LSN
//!    order (guaranteed by `InMemoryWal` and the durable WAL scan).
//! 4. **Tolerate explicit deferrals** — records whose handlers are not
//!    promoted yet are tracked as soft errors; other errors propagate.
//! 5. **Return [`WalReplayReport`]** — counters, LSN bounds, error status.
//!
//! # Idempotency Invariant
//!
//! Replaying the same WAL records twice produces the same final durable
//! state as replaying them once. Every handler in [`super::replay`] is
//! required to uphold this invariant.
//!
//! # LSN Ordering Invariant
//!
//! The redo pass processes records strictly in **ascending LSN order**.
//! `ConceptualRedoPlan.records` is already ordered (derived from `InMemoryWal`
//! or a validated durable WAL scan, both of which guarantee monotonic LSNs).
//!
//! # Transaction Commit Boundary
//!
//! Only records belonging to **committed** transactions (or non-transactional
//! structural records) enter the redo pass. Records belonging to incomplete or
//! rolled-back transactions are discarded by the plan and never reach the
//! replay layer.

use std::collections::HashMap;

use andromeda_core::AndromedaResult;

use crate::{DatabaseManifest, DurableTransactionState, Lsn, WalRecord, WalRecordKind};

use super::planning::{ConceptualRedoPlan, RecoveryPlan, RedoRecordDecision, StartupMode};
use super::replay::{
    IndexRebuildRequiredEvidence, ManifestSwitchRecoveryTrace, ReplayContext, ReplayOutcome,
    ReplayResult, replay_wal_record_result,
};
use super::storage_error;

/// Summary report produced at the end of a single WAL replay session.
///
/// All counters are derived exclusively from the durable WAL prefix and the
/// `ConceptualRedoPlan`. RAM-only state must not be reflected here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalReplayReport {
    /// Recovery startup mode that governed this session.
    pub startup_mode: StartupMode,
    /// LSN from which redo started (manifest's `required_wal_start_lsn`).
    pub replay_start_lsn: Lsn,
    /// Highest LSN where a handler returned `Applied`.
    /// `None` if no record was successfully applied.
    pub replay_end_lsn: Option<Lsn>,
    /// Total plan records evaluated (all decisions combined).
    pub total_records: usize,
    /// Records whose handler returned `Applied`.
    pub applied_count: usize,
    /// Records whose handler returned `Skipped` (informational boundary handlers,
    /// e.g. `CheckpointBegin`, `SecurityAuditAppend`).
    pub handler_skipped_count: usize,
    /// Records whose redo decision was anything other than `Replay`
    /// (`SkipIncompleteTransaction`, `SkipNonRedoRecord`, `SkipBeforeRedoStart`, etc.).
    pub plan_skipped_count: usize,
    /// Records whose handler returned `NotYetImplemented`.
    pub not_yet_implemented_count: usize,
    /// Index/B-Tree records whose payload was validated but whose inline redo
    /// remains gated, requiring an explicit index rebuild before the access
    /// path can be trusted.
    pub index_rebuild_required_count: usize,
    /// Structured access-path rebuild evidence emitted by this replay session.
    ///
    /// Each item binds the access-path object identity (`index_id`), WAL LSN,
    /// transaction evidence, payload checksum, key format, and reason needed by
    /// later catalog or optimizer quarantine/exclusion logic. Storage only
    /// reports the boundary here; it does not update catalog state.
    pub index_rebuild_required: Vec<IndexRebuildRequiredEvidence>,
    /// Incomplete (crash-survivor) transactions identified and discarded by the plan.
    pub incomplete_transaction_count: usize,
    /// Transactions whose `TxCommit` record was found in the durable WAL.
    pub committed_transaction_count: usize,
    /// `true` if any handler returned `NotYetImplemented` or `Deprecated`.
    /// Deferred handlers remain fail-stop at the handler layer and are
    /// reported here rather than treated as fatal driver errors.
    pub has_replay_errors: bool,
}

impl WalReplayReport {
    /// Returns `true` when recovery can open without discarded transactions,
    /// replay errors, or pending access-path rebuilds.
    pub fn is_clean_recovery(&self) -> bool {
        self.incomplete_transaction_count == 0
            && !self.has_replay_errors
            && !self.requires_access_path_rebuild()
    }

    /// Returns `true` when incomplete transactions were discarded.
    pub fn has_discarded_transactions(&self) -> bool {
        self.incomplete_transaction_count > 0
    }

    /// Returns `true` when one or more Index/B-Tree access paths must be
    /// rebuilt or quarantined before the access path can be trusted.
    pub const fn requires_access_path_rebuild(&self) -> bool {
        self.index_rebuild_required_count > 0
    }

    /// Returns structured access-path rebuild evidence emitted by this replay.
    pub fn access_path_rebuild_evidence(&self) -> &[IndexRebuildRequiredEvidence] {
        &self.index_rebuild_required
    }

    /// Returns `true` when any redo records are queued for replay by the plan.
    pub fn has_replay_work(&self) -> bool {
        self.applied_count > 0
            || self.not_yet_implemented_count > 0
            || self.index_rebuild_required_count > 0
    }
}

/// Replay all eligible WAL records starting from the manifest-required LSN.
///
/// This is the primary crash-recovery redo entry point.  The function:
/// 1. Builds a [`ConceptualRedoPlan`] from the manifest and durable records.
/// 2. Drives the redo pass via [`execute_redo_plan`].
/// 3. Returns a [`WalReplayReport`] summarising the session.
///
/// # Arguments
///
/// * `manifest`  — Validated `DatabaseManifest`.  Must pass `validate()`.
/// * `startup_mode` — Controls storage-format gate and incomplete-tx policy.
/// * `durable_records` — Durable WAL records in ascending LSN order, starting
///   at or before `manifest.required_wal_start_lsn`.
///
/// # Errors
///
/// * Manifest validation failure.
/// * Storage format fingerprints incompatible with `startup_mode`.
/// * WAL coverage gap or missing required start LSN.
pub fn replay_wal_from_lsn(
    manifest: &DatabaseManifest,
    startup_mode: StartupMode,
    durable_records: &[WalRecord],
) -> AndromedaResult<WalReplayReport> {
    let mut ctx = ReplayContext::new();
    replay_wal_from_lsn_into_context(manifest, startup_mode, durable_records, &mut ctx)
}

/// Replay all eligible WAL records into a caller-owned recovery context.
///
/// The returned report contains counters for this replay session. The context
/// retains reconstructed heap redo state and replay error evidence for callers
/// that need to validate restart state before opening the database.
pub fn replay_wal_from_lsn_into_context(
    manifest: &DatabaseManifest,
    startup_mode: StartupMode,
    durable_records: &[WalRecord],
    ctx: &mut ReplayContext,
) -> AndromedaResult<WalReplayReport> {
    validate_durable_replay_chain(durable_records)?;
    validate_conflicting_terminal_records(durable_records)?;
    validate_bootstrap_redo_boundary(manifest, durable_records)?;
    let plan = RecoveryPlan::from_manifest_and_wal(manifest, startup_mode, durable_records)?;
    if ctx.active_manifest.is_none() {
        ctx.active_manifest = Some(*manifest);
    }
    execute_redo_plan_into_context(&plan, durable_records, ctx)
}

/// Execute a conceptual redo plan against the supplied durable WAL records.
///
/// This is the inner redo-pass driver.  For each `Replay`-classified record
/// the function calls the replay record handler. `NotYetImplemented` handler
/// errors are accumulated as soft errors in the report; all other errors
/// are propagated to the caller.
///
/// # Correctness Note
///
/// `durable_records` must be the **same slice** (or a superset in LSN order)
/// that was used to build `plan`.  The function looks up each plan LSN in a
/// by-LSN index built from `durable_records`.
pub fn execute_redo_plan(
    plan: &ConceptualRedoPlan,
    durable_records: &[WalRecord],
) -> AndromedaResult<WalReplayReport> {
    let mut ctx = ReplayContext::new();
    execute_redo_plan_into_context(plan, durable_records, &mut ctx)
}

/// Execute a conceptual redo plan into a caller-owned recovery context.
///
/// This variant is used by startup paths that must inspect reconstructed state,
/// such as heap pages rebuilt from promoted HREDOV1 records. The driver rejects
/// mismatched or incomplete plan evidence before invoking a handler.
pub fn execute_redo_plan_into_context(
    plan: &ConceptualRedoPlan,
    durable_records: &[WalRecord],
    ctx: &mut ReplayContext,
) -> AndromedaResult<WalReplayReport> {
    validate_conflicting_terminal_records(durable_records)?;

    // Build O(1) LSN → record index.
    let mut by_lsn: HashMap<Lsn, &WalRecord> = HashMap::with_capacity(durable_records.len());
    for record in durable_records {
        record.validate()?;
        if record.header.kind == WalRecordKind::CheckpointEnd {
            ctx.observe_checkpoint_end(record.header.lsn);
        }
        if by_lsn.insert(record.header.lsn, record).is_some() {
            return Err(storage_error(format!(
                "durable WAL replay input contains duplicate LSN {}",
                record.header.lsn.get()
            )));
        }
    }

    let base_applied_count = ctx.applied_count;
    let base_skipped_count = ctx.skipped_count;
    let base_error_count = ctx.error_records.len();
    let base_index_rebuild_required_count = ctx.index_rebuild_required.len();
    ctx.require_manifest_switch_checkpoint_evidence();

    let mut plan_skipped_count: usize = 0;
    let mut replay_end_lsn: Option<Lsn> = None;
    let mut previous_plan_lsn: Option<Lsn> = None;

    for redo_rec in plan.records.iter() {
        match previous_plan_lsn {
            Some(previous) if redo_rec.lsn <= previous => {
                return Err(storage_error(format!(
                    "redo plan records must be strictly ordered by LSN: previous={}, current={}",
                    previous.get(),
                    redo_rec.lsn.get()
                )));
            }
            _ => {}
        }
        previous_plan_lsn = Some(redo_rec.lsn);

        match redo_rec.decision {
            RedoRecordDecision::Replay => {
                let Some(&record) = by_lsn.get(&redo_rec.lsn) else {
                    return Err(storage_error(format!(
                        "redo plan references LSN {} but the durable WAL input does not contain it",
                        redo_rec.lsn.get()
                    )));
                };
                validate_planned_replay_record(redo_rec, record)?;

                let applied_before = ctx.applied_count;
                let manifest_before = (record.header.kind == WalRecordKind::ManifestSwitch)
                    .then_some(ctx.active_manifest)
                    .flatten();
                let manifest_trace_count_before = ctx.manifest_switch_traces.len();
                match replay_wal_record_result(ctx, record) {
                    Ok(_) => {}
                    Err(_err) if is_explicit_deferred_replay_error(ctx, record) => {}
                    Err(err) => return Err(err),
                }
                if record.header.kind == WalRecordKind::ManifestSwitch {
                    validate_manifest_switch_replay_result(
                        ctx,
                        manifest_before,
                        manifest_trace_count_before,
                    )?;
                }

                if ctx.applied_count > applied_before {
                    replay_end_lsn = Some(record.header.lsn);
                }
            }
            _ => {
                plan_skipped_count += 1;
            }
        }
    }

    let committed_transaction_count = plan
        .transaction_evidence
        .iter()
        .filter(|t| t.commit_lsn.is_some())
        .count();

    let new_index_rebuild_required =
        ctx.index_rebuild_required[base_index_rebuild_required_count..].to_vec();
    let index_rebuild_required_count = new_index_rebuild_required.len();

    Ok(WalReplayReport {
        startup_mode: plan.startup_mode,
        replay_start_lsn: plan.redo_from_lsn,
        replay_end_lsn,
        total_records: plan.records.len(),
        applied_count: ctx.applied_count - base_applied_count,
        handler_skipped_count: ctx.skipped_count - base_skipped_count,
        plan_skipped_count,
        not_yet_implemented_count: ctx.error_records.len() - base_error_count,
        index_rebuild_required_count,
        index_rebuild_required: new_index_rebuild_required,
        incomplete_transaction_count: plan.incomplete_transactions.len(),
        committed_transaction_count,
        has_replay_errors: ctx.error_records.len() > base_error_count,
    })
}

fn validate_durable_replay_chain(durable_records: &[WalRecord]) -> AndromedaResult<()> {
    let mut previous_lsn = None;
    for record in durable_records {
        record.validate()?;
        let Some(previous) = previous_lsn else {
            previous_lsn = Some(record.header.lsn);
            continue;
        };

        if record.header.lsn <= previous {
            return Err(storage_error(format!(
                "durable WAL replay input must be strictly increasing by LSN: previous={}, current={}",
                previous.get(),
                record.header.lsn.get()
            )));
        }

        if record.header.previous_lsn != Some(previous) {
            let observed = record
                .header
                .previous_lsn
                .map_or_else(|| "None".to_string(), |lsn| lsn.get().to_string());
            return Err(storage_error(format!(
                "durable WAL replay input previous LSN chain mismatch at LSN {}: expected previous {}, observed {}",
                record.header.lsn.get(),
                previous.get(),
                observed
            )));
        }

        previous_lsn = Some(record.header.lsn);
    }

    Ok(())
}

fn validate_conflicting_terminal_records(durable_records: &[WalRecord]) -> AndromedaResult<()> {
    let mut terminal_by_transaction: HashMap<u64, (WalRecordKind, Lsn)> = HashMap::new();

    for record in durable_records {
        if !record.is_transaction_terminal() {
            continue;
        }

        let transaction_id = record.header.transaction_id.ok_or_else(|| {
            storage_error("transaction terminal WAL record requires a transaction id")
        })?;
        if let Some((first_kind, first_lsn)) = terminal_by_transaction.insert(
            transaction_id.get(),
            (record.header.kind, record.header.lsn),
        ) {
            return Err(storage_error(format!(
                "durable WAL replay input contains duplicate or conflicting terminal records for transaction {}: first {:?} at LSN {}, second {:?} at LSN {}",
                transaction_id.get(),
                first_kind,
                first_lsn.get(),
                record.header.kind,
                record.header.lsn.get()
            )));
        }
    }

    Ok(())
}

fn validate_bootstrap_redo_boundary(
    manifest: &DatabaseManifest,
    durable_records: &[WalRecord],
) -> AndromedaResult<()> {
    if !manifest.required_wal_start_lsn.is_zero() {
        return Ok(());
    }

    if !manifest.base_checkpoint_lsn.is_zero() {
        return Err(storage_error(
            "bootstrap ZERO redo boundary requires a ZERO base checkpoint; \
             non-bootstrap snapshots must name a nonzero required WAL start LSN",
        ));
    }

    if let Some(first) = durable_records.first()
        && (first.header.lsn != Lsn::new(1) || first.header.previous_lsn.is_some())
    {
        return Err(storage_error(
            "bootstrap ZERO redo boundary requires durable WAL to start at LSN 1 \
             with no previous LSN so recovery cannot skip required WAL",
        ));
    }

    Ok(())
}

fn validate_manifest_switch_replay_result(
    ctx: &ReplayContext,
    previous_manifest: Option<DatabaseManifest>,
    trace_count_before: usize,
) -> AndromedaResult<()> {
    let Some(trace) = ctx.manifest_switch_traces.get(trace_count_before) else {
        return Err(storage_error(
            "ManifestSwitch replay produced no recovery trace; recovery cannot open without observable switch evidence",
        ));
    };

    match *trace {
        ManifestSwitchRecoveryTrace::ManifestSwitchValidationFailed {
            lsn,
            manifest_version,
            reason,
        } => Err(storage_error(format!(
            "ManifestSwitch at LSN {} for manifest version {} failed validation: {}",
            lsn.get(),
            manifest_version,
            reason
        ))),
        ManifestSwitchRecoveryTrace::ManifestSwitchApplied {
            lsn,
            manifest_version,
            base_checkpoint_lsn,
            required_wal_start_lsn,
            ..
        } => {
            if let Some(previous) = previous_manifest {
                if manifest_version <= previous.manifest_version {
                    return Err(storage_error(format!(
                        "ManifestSwitch at LSN {} regresses manifest version: previous={}, next={}",
                        lsn.get(),
                        previous.manifest_version,
                        manifest_version
                    )));
                }
                if base_checkpoint_lsn <= previous.base_checkpoint_lsn {
                    return Err(storage_error(format!(
                        "ManifestSwitch at LSN {} must strictly advance base checkpoint LSN: previous={}, next={}",
                        lsn.get(),
                        previous.base_checkpoint_lsn.get(),
                        base_checkpoint_lsn.get()
                    )));
                }
                if required_wal_start_lsn < previous.required_wal_start_lsn {
                    return Err(storage_error(format!(
                        "ManifestSwitch at LSN {} regresses required WAL start LSN: previous={}, next={}",
                        lsn.get(),
                        previous.required_wal_start_lsn.get(),
                        required_wal_start_lsn.get()
                    )));
                }
                if lsn <= previous.base_checkpoint_lsn {
                    return Err(storage_error(format!(
                        "ManifestSwitch record LSN {} must follow the previous base checkpoint LSN {}",
                        lsn.get(),
                        previous.base_checkpoint_lsn.get()
                    )));
                }
            }
            Ok(())
        }
    }
}

fn validate_planned_replay_record(
    redo_rec: &super::planning::RedoRecordPlan,
    record: &WalRecord,
) -> AndromedaResult<()> {
    if record.header.kind != redo_rec.kind {
        return Err(storage_error(format!(
            "redo plan LSN {} expects {:?} but durable WAL contains {:?}",
            redo_rec.lsn.get(),
            redo_rec.kind,
            record.header.kind
        )));
    }
    if record.header.transaction_id != redo_rec.transaction_id {
        return Err(storage_error(format!(
            "redo plan LSN {} transaction evidence does not match durable WAL",
            redo_rec.lsn.get()
        )));
    }
    match (record.header.transaction_id, redo_rec.transaction_state) {
        (Some(_), Some(DurableTransactionState::Committed)) => Ok(()),
        (Some(_), Some(state)) => Err(storage_error(format!(
            "redo plan attempted to replay non-committed transaction record at LSN {}: state={state:?}",
            redo_rec.lsn.get()
        ))),
        (Some(_), None) => Err(storage_error(format!(
            "redo plan attempted to replay transaction record at LSN {} without commit evidence",
            redo_rec.lsn.get()
        ))),
        (None, _) => Ok(()),
    }
}

fn is_explicit_deferred_replay_error(ctx: &ReplayContext, record: &WalRecord) -> bool {
    let Some(result) = ctx.error_records.last() else {
        return false;
    };
    result.lsn == record.header.lsn
        && result.kind == record.header.kind
        && result.outcome == ReplayOutcome::NotYetImplemented
        && is_explicit_deferred_kind(result)
}

fn is_explicit_deferred_kind(result: &ReplayResult) -> bool {
    match result.kind {
        WalRecordKind::PageAllocate
        | WalRecordKind::PageFormat
        | WalRecordKind::IndexInsert
        | WalRecordKind::IndexDelete
        | WalRecordKind::MvccVersionCreate
        | WalRecordKind::MvccVersionClose
        | WalRecordKind::MapDeltaAppend
        | WalRecordKind::CatalogChangeBegin
        | WalRecordKind::CatalogChangeApply
        | WalRecordKind::CatalogChangeCommit
        | WalRecordKind::BTreeInsert
        | WalRecordKind::BTreeDelete
        | WalRecordKind::BTreeSplit
        | WalRecordKind::BTreeMerge => result
            .error
            .as_deref()
            .is_some_and(|message| message.contains("not promoted")),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::TransactionId;

    use crate::{DatabaseManifest, InMemoryWal, WalRecordKind};

    fn test_manifest(required_wal_start_lsn: Lsn) -> DatabaseManifest {
        DatabaseManifest {
            database_id: 1,
            manifest_version: 1,
            snapshot_id: 1,
            base_checkpoint_lsn: Lsn::ZERO,
            required_wal_start_lsn,
            previous_manifest_hash: [0; 32],
            manifest_crc: 0xdead_beef,
        }
    }

    #[test]
    fn test_replay_empty_wal_zero_lsn_anchor() {
        let manifest = test_manifest(Lsn::ZERO);
        let report = replay_wal_from_lsn(&manifest, StartupMode::SafeStart, &[]).unwrap();
        assert_eq!(report.total_records, 0);
        assert_eq!(report.applied_count, 0);
        assert_eq!(report.incomplete_transaction_count, 0);
        assert_eq!(report.committed_transaction_count, 0);
        assert!(!report.has_replay_errors);
        assert!(report.replay_end_lsn.is_none());
    }

    #[test]
    fn test_replay_committed_tx_row_insert_classified_replay() {
        let tx = TransactionId::new(10);
        let mut wal = InMemoryWal::new();
        wal.append_tx_begin(tx).unwrap();
        wal.append_payload(WalRecordKind::RowInsert, Some(tx), b"row_data")
            .unwrap();
        wal.append_tx_commit(tx).unwrap();
        wal.flush_all().unwrap();

        let records = wal.replay_durable();
        let manifest = test_manifest(Lsn::new(1));
        let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
            .unwrap();

        let row_rec = plan
            .records
            .iter()
            .find(|r| r.kind == WalRecordKind::RowInsert)
            .expect("RowInsert must be in plan");
        assert_eq!(row_rec.decision, RedoRecordDecision::Replay);
        assert!(!plan.has_incomplete_transactions());
    }

    #[test]
    fn test_replay_incomplete_tx_row_insert_skip_incomplete() {
        let tx = TransactionId::new(10);
        let mut wal = InMemoryWal::new();
        wal.append_tx_begin(tx).unwrap();
        wal.append_payload(WalRecordKind::RowInsert, Some(tx), b"row_data")
            .unwrap();
        // crash — no TxCommit
        wal.flush_all().unwrap();

        let records = wal.replay_durable();
        let manifest = test_manifest(Lsn::new(1));
        let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
            .unwrap();

        assert!(plan.has_incomplete_transactions());
        let row_rec = plan
            .records
            .iter()
            .find(|r| r.kind == WalRecordKind::RowInsert)
            .expect("RowInsert must be in plan");
        assert_eq!(
            row_rec.decision,
            RedoRecordDecision::SkipIncompleteTransaction
        );
    }

    #[test]
    fn test_replay_rolled_back_tx_row_insert_skip_rolled_back() {
        let tx = TransactionId::new(10);
        let mut wal = InMemoryWal::new();
        wal.append_tx_begin(tx).unwrap();
        wal.append_payload(WalRecordKind::RowInsert, Some(tx), b"row_data")
            .unwrap();
        wal.append_tx_rollback(tx).unwrap();
        wal.flush_all().unwrap();

        let records = wal.replay_durable();
        let manifest = test_manifest(Lsn::new(1));
        let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
            .unwrap();

        let row_rec = plan
            .records
            .iter()
            .find(|r| r.kind == WalRecordKind::RowInsert)
            .expect("RowInsert must be in plan");
        assert_eq!(
            row_rec.decision,
            RedoRecordDecision::SkipRolledBackTransaction
        );
    }

    #[test]
    fn test_replay_deferred_handlers_are_soft_errors_not_fatal() {
        let tx = TransactionId::new(10);
        let mut wal = InMemoryWal::new();
        wal.append_tx_begin(tx).unwrap();
        wal.append_payload(WalRecordKind::MvccVersionCreate, Some(tx), b"mvcc")
            .unwrap();
        wal.append_tx_commit(tx).unwrap();
        wal.flush_all().unwrap();

        let records = wal.replay_durable();
        let manifest = test_manifest(Lsn::new(1));
        let report = replay_wal_from_lsn(&manifest, StartupMode::SafeStart, &records).unwrap();
        assert_eq!(report.not_yet_implemented_count, 1);
        assert_eq!(report.applied_count, 0);
        assert_eq!(report.committed_transaction_count, 1);
        assert_eq!(report.incomplete_transaction_count, 0);
        assert!(report.has_replay_errors);
        assert!(!report.is_clean_recovery());
    }

    #[test]
    fn test_replay_malformed_promoted_heap_payload_is_fatal() {
        let tx = TransactionId::new(10);
        let mut wal = InMemoryWal::new();
        wal.append_tx_begin(tx).unwrap();
        wal.append_payload(WalRecordKind::RowInsert, Some(tx), b"row_data")
            .unwrap();
        wal.append_tx_commit(tx).unwrap();
        wal.flush_all().unwrap();

        let records = wal.replay_durable();
        let manifest = test_manifest(Lsn::new(1));
        let err = replay_wal_from_lsn(&manifest, StartupMode::SafeStart, &records)
            .expect_err("malformed promoted heap redo must fail closed");

        assert!(err.message().contains("HREDOV1"));
        assert!(err.message().contains("fail closed"));
    }

    #[test]
    fn test_execute_redo_plan_rejects_missing_planned_lsn() {
        let tx = TransactionId::new(10);
        let mut wal = InMemoryWal::new();
        wal.append_tx_begin(tx).unwrap();
        wal.append_payload(WalRecordKind::MvccVersionCreate, Some(tx), b"mvcc")
            .unwrap();
        wal.append_tx_commit(tx).unwrap();
        wal.flush_all().unwrap();

        let records = wal.replay_durable();
        let manifest = test_manifest(Lsn::new(1));
        let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
            .unwrap();
        let truncated_records = vec![records[0].clone(), records[2].clone()];

        let err = execute_redo_plan(&plan, &truncated_records)
            .expect_err("executor must reject a plan/slice mismatch");

        assert!(err.message().contains("redo plan references LSN 2"));
        assert!(err.message().contains("does not contain it"));
    }

    #[test]
    fn test_replay_multiple_committed_txns_all_replayed() {
        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);
        let mut wal = InMemoryWal::new();
        wal.append_tx_begin(tx1).unwrap();
        wal.append_payload(WalRecordKind::RowInsert, Some(tx1), b"row_1")
            .unwrap();
        wal.append_tx_commit(tx1).unwrap();
        wal.append_tx_begin(tx2).unwrap();
        wal.append_payload(WalRecordKind::RowUpdate, Some(tx2), b"row_2")
            .unwrap();
        wal.append_tx_commit(tx2).unwrap();
        wal.flush_all().unwrap();

        let records = wal.replay_durable();
        let manifest = test_manifest(Lsn::new(1));
        let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
            .unwrap();

        let replay_decisions: Vec<_> = plan
            .records
            .iter()
            .filter(|r| r.kind == WalRecordKind::RowInsert || r.kind == WalRecordKind::RowUpdate)
            .map(|r| r.decision)
            .collect();
        assert_eq!(replay_decisions.len(), 2);
        assert!(
            replay_decisions
                .iter()
                .all(|d| *d == RedoRecordDecision::Replay)
        );
    }

    #[test]
    fn test_replay_btree_insert_committed_tx_enters_replay_gate() {
        let tx = TransactionId::new(10);
        let mut wal = InMemoryWal::new();
        wal.append_tx_begin(tx).unwrap();
        wal.append_payload(WalRecordKind::BTreeInsert, Some(tx), b"btree")
            .unwrap();
        wal.append_tx_commit(tx).unwrap();
        wal.flush_all().unwrap();

        let records = wal.replay_durable();
        let manifest = test_manifest(Lsn::new(1));
        let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
            .unwrap();

        let btree_rec = plan
            .records
            .iter()
            .find(|r| r.kind == WalRecordKind::BTreeInsert)
            .expect("BTreeInsert must be in plan");
        assert_eq!(btree_rec.decision, RedoRecordDecision::Replay);
    }

    #[test]
    fn test_replay_recovered_transaction_id_floor() {
        let tx = TransactionId::new(999);
        let mut wal = InMemoryWal::new();
        wal.append_tx_begin(tx).unwrap();
        wal.append_tx_commit(tx).unwrap();
        wal.flush_all().unwrap();

        let records = wal.replay_durable();
        let manifest = test_manifest(Lsn::new(1));
        let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
            .unwrap();

        assert_eq!(plan.recovered_transaction_id_floor(), 999);
    }
}
