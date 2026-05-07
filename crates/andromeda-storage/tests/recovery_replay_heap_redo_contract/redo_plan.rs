use crate::support::{
    append_hredov1_delete, append_hredov1_insert, append_hredov1_update, test_manifest,
};
use andromeda_core::TransactionId;
use andromeda_storage::{
    InMemoryWal, Lsn, PageId, RecoveryPlan, ReplayContext, StartupMode,
    execute_redo_plan_into_context,
};

#[test]
fn redo_plan_reconstructs_only_committed_hredov1_heap_state() {
    let page_id = PageId::new(707);
    let tx = TransactionId::new(18);
    let mut wal = InMemoryWal::new();

    wal.append_tx_begin(tx).expect("begin");
    append_hredov1_insert(&mut wal, tx, page_id, 0, Lsn::ZERO, b"old");
    append_hredov1_update(&mut wal, tx, page_id, 0, 1, Lsn::new(2), b"new");
    append_hredov1_delete(&mut wal, tx, page_id, 1, Lsn::new(3));
    wal.append_tx_commit(tx).expect("commit");
    wal.flush_all().expect("durable commit");

    let records = wal.replay_durable();
    let manifest = test_manifest(Lsn::new(1));
    let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
        .expect("redo plan");
    let mut ctx = ReplayContext::new();

    let report =
        execute_redo_plan_into_context(&plan, &records, &mut ctx).expect("committed heap redo");

    assert_eq!(report.applied_count, 3);
    assert_eq!(report.plan_skipped_count, 2);
    assert_eq!(report.committed_transaction_count, 1);
    assert_eq!(report.incomplete_transaction_count, 0);
    assert_eq!(report.replay_end_lsn, Some(Lsn::new(4)));
    assert!(!report.has_replay_errors);

    let page = ctx.heap_redo_page(page_id).expect("recovered heap page");
    assert_eq!(page.page_lsn(), Lsn::new(4));
    assert!(page.is_slot_deleted(0));
    assert!(page.is_slot_deleted(1));
    assert_eq!(page.live_slot_count(), 0);
}

#[test]
fn redo_plan_discards_incomplete_hredov1_heap_records_without_touching_state() {
    let page_id = PageId::new(708);
    let tx = TransactionId::new(19);
    let mut wal = InMemoryWal::new();

    wal.append_tx_begin(tx).expect("begin");
    append_hredov1_insert(&mut wal, tx, page_id, 0, Lsn::ZERO, b"uncommitted");
    wal.flush_all().expect("durable crash survivor");

    let records = wal.replay_durable();
    let manifest = test_manifest(Lsn::new(1));
    let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
        .expect("redo plan");
    let mut ctx = ReplayContext::new();

    let report = execute_redo_plan_into_context(&plan, &records, &mut ctx)
        .expect("incomplete transaction must be discarded by plan");

    assert_eq!(report.applied_count, 0);
    assert_eq!(report.plan_skipped_count, 2);
    assert_eq!(report.incomplete_transaction_count, 1);
    assert!(report.has_discarded_transactions());
    assert!(!report.has_replay_errors);
    assert_eq!(ctx.heap_redo_page_count(), 0);
}

#[test]
fn redo_plan_ignores_heap_commit_that_was_not_in_durable_wal_prefix() {
    let page_id = PageId::new(710);
    let tx = TransactionId::new(22);
    let mut wal = InMemoryWal::new();

    wal.append_tx_begin(tx).expect("begin");
    append_hredov1_insert(&mut wal, tx, page_id, 0, Lsn::ZERO, b"not-durable");
    wal.append_tx_commit(tx)
        .expect("commit appended but not flushed");

    let records = wal.replay_durable();
    assert!(
        records.is_empty(),
        "unflushed WAL records must not be recoverable system truth"
    );
    let manifest_requiring_wal = test_manifest(Lsn::new(1));
    let error = RecoveryPlan::from_manifest_and_wal(
        &manifest_requiring_wal,
        StartupMode::SafeStart,
        &records,
    )
    .expect_err("missing required WAL coverage must fail closed");
    assert!(error.message().contains("WAL coverage"));
    assert!(error.message().contains("required WAL start LSN"));

    let manifest_without_redo = test_manifest(Lsn::ZERO);
    let plan = RecoveryPlan::from_manifest_and_wal(
        &manifest_without_redo,
        StartupMode::SafeStart,
        &records,
    )
    .expect("empty durable prefix is valid only when manifest requires no WAL replay");
    let mut ctx = ReplayContext::new();

    let report =
        execute_redo_plan_into_context(&plan, &records, &mut ctx).expect("empty redo replay");

    assert_eq!(report.applied_count, 0);
    assert_eq!(report.committed_transaction_count, 0);
    assert_eq!(report.incomplete_transaction_count, 0);
    assert_eq!(report.replay_end_lsn, None);
    assert!(!report.has_replay_errors);
    assert_eq!(ctx.heap_redo_page_count(), 0);
}

#[test]
fn redo_plan_applies_committed_heap_and_skips_trailing_incomplete_update() {
    let page_id = PageId::new(709);
    let committed_tx = TransactionId::new(20);
    let incomplete_tx = TransactionId::new(21);
    let mut wal = InMemoryWal::new();

    wal.append_tx_begin(committed_tx).expect("begin committed");
    append_hredov1_insert(&mut wal, committed_tx, page_id, 0, Lsn::ZERO, b"committed");
    wal.append_tx_commit(committed_tx)
        .expect("commit committed");

    wal.append_tx_begin(incomplete_tx)
        .expect("begin incomplete");
    append_hredov1_update(
        &mut wal,
        incomplete_tx,
        page_id,
        0,
        1,
        Lsn::new(2),
        b"uncommitted-update",
    );
    wal.flush_all().expect("durable crash survivor");

    let records = wal.replay_durable();
    let manifest = test_manifest(Lsn::new(1));
    let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
        .expect("redo plan");
    let mut ctx = ReplayContext::new();

    let report = execute_redo_plan_into_context(&plan, &records, &mut ctx).expect("mixed replay");

    assert_eq!(report.applied_count, 1);
    assert_eq!(report.incomplete_transaction_count, 1);
    assert_eq!(report.committed_transaction_count, 1);
    assert!(!report.has_replay_errors);

    let page = ctx.heap_redo_page(page_id).expect("committed page");
    assert_eq!(page.page_lsn(), Lsn::new(2));
    assert_eq!(page.read_tuple(0), Some(b"committed".as_slice()));
    assert_eq!(page.read_tuple(1), None);
    assert_eq!(page.live_slot_count(), 1);
}
