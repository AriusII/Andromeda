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

use crate::{DatabaseManifest, Lsn, WalRecord};

use super::planning::{ConceptualRedoPlan, RecoveryPlan, RedoRecordDecision, StartupMode};
use super::replay::{ReplayContext, replay_wal_record};

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
    /// Returns `true` when the replay completed with no incomplete
    /// transactions and no replay errors.
    pub fn is_clean_recovery(&self) -> bool {
        self.incomplete_transaction_count == 0 && !self.has_replay_errors
    }

    /// Returns `true` when incomplete transactions were discarded.
    pub fn has_discarded_transactions(&self) -> bool {
        self.incomplete_transaction_count > 0
    }

    /// Returns `true` when any redo records are queued for replay by the plan.
    pub fn has_replay_work(&self) -> bool {
        self.applied_count > 0 || self.not_yet_implemented_count > 0
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
    let plan = RecoveryPlan::from_manifest_and_wal(manifest, startup_mode, durable_records)?;
    execute_redo_plan(&plan, durable_records)
}

/// Execute a conceptual redo plan against the supplied durable WAL records.
///
/// This is the inner redo-pass driver.  For each `Replay`-classified record
/// the function calls [`replay_wal_record`].  `NotYetImplemented` handler
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
    // Build O(1) LSN → record index.
    let by_lsn: HashMap<Lsn, &WalRecord> =
        durable_records.iter().map(|r| (r.header.lsn, r)).collect();

    let mut ctx = ReplayContext::new();
    let mut plan_skipped_count: usize = 0;
    let mut replay_end_lsn: Option<Lsn> = None;

    for redo_rec in plan.records.iter() {
        match redo_rec.decision {
            RedoRecordDecision::Replay => {
                let Some(&record) = by_lsn.get(&redo_rec.lsn) else {
                    // Record listed in plan but absent from durable slice.
                    // This indicates a WAL coverage invariant violation.
                    continue;
                };

                let applied_before = ctx.applied_count;
                // `replay_wal_record` pushes to `ctx.error_records` before
                // returning Err, so we can safely ignore the returned Err
                // and interrogate ctx afterwards.
                let _ = replay_wal_record(&mut ctx, record);

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

    Ok(WalReplayReport {
        startup_mode: plan.startup_mode,
        replay_start_lsn: plan.redo_from_lsn,
        replay_end_lsn,
        total_records: plan.records.len(),
        applied_count: ctx.applied_count,
        handler_skipped_count: ctx.skipped_count,
        plan_skipped_count,
        not_yet_implemented_count: ctx.error_records.len(),
        incomplete_transaction_count: plan.incomplete_transactions.len(),
        committed_transaction_count,
        has_replay_errors: ctx.has_errors(),
    })
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
        wal.append_payload(WalRecordKind::RowInsert, Some(tx), b"row_data")
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
    fn test_replay_btree_insert_always_skip_non_redo() {
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
        assert_eq!(btree_rec.decision, RedoRecordDecision::SkipNonRedoRecord);
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
