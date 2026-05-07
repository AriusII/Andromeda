//! Page WAL replay promotion-gate contracts.
//!
//! PageAllocate/PageFormat are redo-relevant records, but their durable payload
//! schema and replay apply target are not promoted yet. Recovery must fail
//! closed for both empty and non-empty payloads instead of inferring a format.

use andromeda_core::{AndromedaErrorKind, TransactionId};
use andromeda_storage::{
    DatabaseManifest, InMemoryWal, Lsn, RecoveryPlan, ReplayContext, ReplayOutcome, StartupMode,
    WalRecord, WalRecordKind, execute_redo_plan_into_context, replay_wal_record,
};

#[test]
fn page_allocate_replay_fails_closed_until_payload_and_apply_contract_exist() {
    assert_page_record_fails_closed(
        WalRecordKind::PageAllocate,
        Lsn::new(10),
        Vec::new(),
        "durable PageAllocate payload schema",
    );
    assert_page_record_fails_closed(
        WalRecordKind::PageAllocate,
        Lsn::new(11),
        b"page-id=42;allocation-id=7".to_vec(),
        "durable PageAllocate payload schema",
    );
}

#[test]
fn page_format_replay_fails_closed_until_payload_and_apply_contract_exist() {
    assert_page_record_fails_closed(
        WalRecordKind::PageFormat,
        Lsn::new(20),
        Vec::new(),
        "durable PageFormat payload schema",
    );
    assert_page_record_fails_closed(
        WalRecordKind::PageFormat,
        Lsn::new(21),
        b"page-id=42;format=v1;page-type=fixed-row".to_vec(),
        "durable PageFormat payload schema",
    );
}

#[test]
fn future_page_payload_envelopes_fail_closed_until_full_promotion_gates_exist() {
    assert_page_record_fails_closed(
        WalRecordKind::PageAllocate,
        Lsn::new(30),
        b"PALLOCV1;page_id=42;extent_id=7;schema=stable;golden=present;prop_fuzz=present;crash_recovery=present"
            .to_vec(),
        "durable PageAllocate payload schema",
    );
    assert_page_record_fails_closed(
        WalRecordKind::PageFormat,
        Lsn::new(31),
        b"PFRMTV1;page_id=42;page_size=16384;codec=PGV1;golden=present;prop_fuzz=present;crash_recovery=present"
            .to_vec(),
        "durable PageFormat payload schema",
    );
}

#[test]
fn redo_plan_reports_nontransactional_page_records_as_explicit_replay_gates() {
    let mut wal = InMemoryWal::new();
    wal.append_payload(WalRecordKind::PageAllocate, None, b"page-allocate")
        .expect("append page allocate");
    wal.append_payload(WalRecordKind::PageFormat, None, b"page-format")
        .expect("append page format");
    wal.flush_all().expect("durable page records");

    let records = wal.replay_durable();
    let manifest = test_manifest(Lsn::new(1));
    let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
        .expect("redo plan");
    let mut ctx = ReplayContext::new();

    let report = execute_redo_plan_into_context(&plan, &records, &mut ctx)
        .expect("explicit page gates are reported as replay errors");

    assert_eq!(report.total_records, 2);
    assert_eq!(report.applied_count, 0);
    assert_eq!(report.plan_skipped_count, 0);
    assert_eq!(report.not_yet_implemented_count, 2);
    assert!(report.has_replay_errors);
    assert_eq!(ctx.error_records.len(), 2);
    assert!(
        ctx.error_records
            .iter()
            .all(|result| result.error.as_deref().is_some_and(|message| {
                message.contains("not promoted") && message.contains("fail closed")
            }))
    );
}

#[test]
fn transaction_scoped_page_record_without_commit_is_discarded_before_handler_gate() {
    let tx = TransactionId::new(501);
    let mut wal = InMemoryWal::new();
    wal.append_tx_begin(tx).expect("begin");
    wal.append_payload(WalRecordKind::PageFormat, Some(tx), b"page-format")
        .expect("append transaction-scoped page format");
    wal.flush_all().expect("durable incomplete transaction");

    let records = wal.replay_durable();
    let manifest = test_manifest(Lsn::new(1));
    let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
        .expect("redo plan");
    let mut ctx = ReplayContext::new();

    let report = execute_redo_plan_into_context(&plan, &records, &mut ctx)
        .expect("incomplete page record must be plan-discarded");

    assert_eq!(report.applied_count, 0);
    assert_eq!(report.plan_skipped_count, 2);
    assert_eq!(report.incomplete_transaction_count, 1);
    assert_eq!(report.not_yet_implemented_count, 0);
    assert!(!report.has_replay_errors);
    assert!(ctx.error_records.is_empty());
}

#[test]
fn transaction_scoped_page_record_with_commit_reaches_fail_closed_gate() {
    let tx = TransactionId::new(502);
    let mut wal = InMemoryWal::new();
    wal.append_tx_begin(tx).expect("begin");
    wal.append_payload(WalRecordKind::PageFormat, Some(tx), b"page-format")
        .expect("append transaction-scoped page format");
    wal.append_tx_commit(tx).expect("commit");
    wal.flush_all().expect("durable commit");

    let records = wal.replay_durable();
    let manifest = test_manifest(Lsn::new(1));
    let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
        .expect("redo plan");
    let mut ctx = ReplayContext::new();

    let report = execute_redo_plan_into_context(&plan, &records, &mut ctx)
        .expect("committed page record gate should be reported");

    assert_eq!(report.applied_count, 0);
    assert_eq!(report.plan_skipped_count, 2);
    assert_eq!(report.committed_transaction_count, 1);
    assert_eq!(report.not_yet_implemented_count, 1);
    assert!(report.has_replay_errors);
    assert_eq!(ctx.error_records[0].kind, WalRecordKind::PageFormat);
}

fn assert_page_record_fails_closed(
    kind: WalRecordKind,
    lsn: Lsn,
    payload: Vec<u8>,
    missing_contract: &str,
) {
    let record = WalRecord::from_parts(kind, lsn, None, None, payload)
        .expect("page WAL record should be structurally valid");
    let mut ctx = ReplayContext::new();

    let err = replay_wal_record(&mut ctx, &record)
        .expect_err("page replay must fail closed until promoted");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert_eq!(ctx.last_replayed_lsn, Some(lsn));
    assert_eq!(ctx.applied_count, 0);
    assert_eq!(ctx.skipped_count, 0);
    assert_eq!(ctx.error_records.len(), 1);

    let error_record = &ctx.error_records[0];
    assert_eq!(error_record.lsn, lsn);
    assert_eq!(error_record.kind, kind);
    assert_eq!(error_record.outcome, ReplayOutcome::NotYetImplemented);
    assert_eq!(
        error_record.error.as_deref(),
        Some(err.message()),
        "returned error and replay context must expose the same fail-closed gate",
    );

    let message = err.message();
    assert!(
        message.contains(&format!("{kind:?}")),
        "gate error must identify the record kind: {message}"
    );
    assert!(
        message.contains("not promoted"),
        "gate error must identify the promotion boundary: {message}"
    );
    assert!(
        message.contains(missing_contract),
        "gate error must document the missing page replay contract: {message}"
    );
    assert!(
        message.contains("fail closed") && message.contains("idempotent"),
        "gate error must reject inferred formats and require idempotent replay: {message}"
    );
}

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
