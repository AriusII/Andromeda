use crate::support::{
    assert_missing_snapshot_base_state, record, row_delete_record, row_insert_record,
    row_update_record,
};
use andromeda_core::{AndromedaErrorKind, TransactionId};
use andromeda_storage::{
    Lsn, PageId, ReplayContext, ReplayOutcome, WalRecordKind, replay_wal_record,
};

#[test]
fn row_insert_hredov1_applies_tuple_and_is_idempotent() {
    let page_id = PageId::new(700);
    let tx = TransactionId::new(10);
    let record = row_insert_record(Lsn::new(10), tx, page_id, 2, Lsn::ZERO, b"alpha");
    let mut ctx = ReplayContext::new();

    replay_wal_record(&mut ctx, &record).expect("first insert replay");
    replay_wal_record(&mut ctx, &record).expect("duplicate insert replay must be idempotent");

    assert_eq!(ctx.applied_count, 2);
    assert_eq!(ctx.heap_redo_page_count(), 1);
    assert_eq!(ctx.heap_redo_page_origin(page_id), Some("redo-created"));
    assert!(!ctx.has_errors());

    let page = ctx
        .heap_redo_page(page_id)
        .expect("heap redo page should exist");
    assert_eq!(page.page_lsn(), Lsn::new(10));
    assert_eq!(page.slot_count(), 1);
    assert_eq!(page.live_slot_count(), 1);
    assert_eq!(page.read_tuple(2), Some(b"alpha".as_slice()));
    assert_eq!(
        page.read_tuple_with_lsn(2),
        Some((b"alpha".as_slice(), Lsn::new(10)))
    );
}

#[test]
fn row_insert_conflicting_same_slot_tuple_fails_stop() {
    let page_id = PageId::new(701);
    let tx = TransactionId::new(11);
    let insert = row_insert_record(Lsn::new(20), tx, page_id, 0, Lsn::ZERO, b"alpha");
    let conflicting_insert = row_insert_record(Lsn::new(21), tx, page_id, 0, Lsn::new(20), b"beta");
    let mut ctx = ReplayContext::new();

    replay_wal_record(&mut ctx, &insert).expect("initial insert replay");
    let err = replay_wal_record(&mut ctx, &conflicting_insert)
        .expect_err("conflicting insert must fail closed");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(err.message().contains("RowInsert"));
    assert!(err.message().contains("conflict"));
    assert_eq!(ctx.error_records.len(), 1);
    assert_eq!(
        ctx.error_records[0].outcome,
        ReplayOutcome::NotYetImplemented
    );
    assert_eq!(
        ctx.heap_redo_page(page_id)
            .expect("page should remain")
            .read_tuple(0),
        Some(b"alpha".as_slice())
    );
}

#[test]
fn row_delete_marks_slot_deleted_and_is_idempotent() {
    let page_id = PageId::new(702);
    let tx = TransactionId::new(12);
    let insert = row_insert_record(Lsn::new(30), tx, page_id, 4, Lsn::ZERO, b"victim");
    let delete = row_delete_record(Lsn::new(31), tx, page_id, 4, Lsn::new(30));
    let mut ctx = ReplayContext::new();

    replay_wal_record(&mut ctx, &insert).expect("insert replay");
    replay_wal_record(&mut ctx, &delete).expect("delete replay");
    replay_wal_record(&mut ctx, &delete).expect("duplicate delete replay must be idempotent");

    let page = ctx.heap_redo_page(page_id).expect("page exists");
    assert_eq!(page.page_lsn(), Lsn::new(31));
    assert_eq!(page.live_slot_count(), 0);
    assert!(page.is_slot_deleted(4));
    assert_eq!(page.read_tuple(4), None);
    assert!(!ctx.has_errors());
}

#[test]
fn row_delete_unknown_slot_fails_stop() {
    let page_id = PageId::new(703);
    let tx = TransactionId::new(13);
    let delete = row_delete_record(Lsn::new(40), tx, page_id, 7, Lsn::ZERO);
    let mut ctx = ReplayContext::new();

    let err =
        replay_wal_record(&mut ctx, &delete).expect_err("delete of unknown slot must fail stop");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert_missing_snapshot_base_state(&ctx, page_id, err.message());
}

#[test]
fn row_update_unknown_old_slot_fails_without_creating_heap_redo_page() {
    let page_id = PageId::new(711);
    let tx = TransactionId::new(23);
    let update = row_update_record(Lsn::new(41), tx, page_id, 0, 1, Lsn::ZERO, b"new");
    let mut ctx = ReplayContext::new();

    let err =
        replay_wal_record(&mut ctx, &update).expect_err("update without prior page must fail stop");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert_missing_snapshot_base_state(&ctx, page_id, err.message());
}

#[test]
fn row_update_closes_old_slot_and_inserts_new_version_idempotently() {
    let page_id = PageId::new(704);
    let tx = TransactionId::new(14);
    let insert = row_insert_record(Lsn::new(50), tx, page_id, 0, Lsn::ZERO, b"old");
    let update = row_update_record(Lsn::new(51), tx, page_id, 0, 1, Lsn::new(50), b"new");
    let mut ctx = ReplayContext::new();

    replay_wal_record(&mut ctx, &insert).expect("insert replay");
    replay_wal_record(&mut ctx, &update).expect("update replay");
    replay_wal_record(&mut ctx, &update).expect("duplicate update replay must be idempotent");

    let page = ctx.heap_redo_page(page_id).expect("page exists");
    assert_eq!(page.page_lsn(), Lsn::new(51));
    assert!(page.is_slot_deleted(0));
    assert_eq!(page.read_tuple(0), None);
    assert_eq!(page.read_tuple(1), Some(b"new".as_slice()));
    assert_eq!(page.live_slot_count(), 1);
    assert!(!ctx.has_errors());
}

#[test]
fn row_update_new_slot_conflict_fails_stop() {
    let page_id = PageId::new(705);
    let tx = TransactionId::new(15);
    let mut ctx = ReplayContext::new();
    replay_wal_record(
        &mut ctx,
        &row_insert_record(Lsn::new(60), tx, page_id, 0, Lsn::ZERO, b"old"),
    )
    .expect("old insert replay");
    replay_wal_record(
        &mut ctx,
        &row_insert_record(Lsn::new(61), tx, page_id, 1, Lsn::new(60), b"occupied"),
    )
    .expect("occupied insert replay");

    let update = row_update_record(Lsn::new(62), tx, page_id, 0, 1, Lsn::new(61), b"new");
    let err = replay_wal_record(&mut ctx, &update)
        .expect_err("update into occupied different slot must fail closed");

    assert!(err.message().contains("RowUpdate"));
    assert!(err.message().contains("different tuple bytes"));
    assert_eq!(
        ctx.heap_redo_page(page_id)
            .expect("page should remain")
            .read_tuple(1),
        Some(b"occupied".as_slice())
    );
}

#[test]
fn unversioned_row_payload_fails_closed_without_inferred_format() {
    let tx = TransactionId::new(16);
    let record = record(
        WalRecordKind::RowInsert,
        Lsn::new(70),
        tx,
        b"legacy-row-bytes".to_vec(),
    );
    let mut ctx = ReplayContext::new();

    let err =
        replay_wal_record(&mut ctx, &record).expect_err("unversioned row redo must fail closed");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(err.message().contains("HREDOV1"));
    assert!(err.message().contains("not promoted"));
    assert!(err.message().contains("fail closed"));
    assert_eq!(ctx.heap_redo_page_count(), 0);
}

#[test]
fn same_heap_redo_sequence_replays_deterministically_in_fresh_contexts() {
    let page_id = PageId::new(706);
    let tx = TransactionId::new(17);
    let records = vec![
        row_insert_record(Lsn::new(80), tx, page_id, 0, Lsn::ZERO, b"a"),
        row_update_record(Lsn::new(81), tx, page_id, 0, 1, Lsn::new(80), b"b"),
        row_delete_record(Lsn::new(82), tx, page_id, 1, Lsn::new(81)),
    ];

    let mut left = ReplayContext::new();
    let mut right = ReplayContext::new();
    for record in &records {
        replay_wal_record(&mut left, record).expect("left replay");
        replay_wal_record(&mut right, record).expect("right replay");
    }

    assert_eq!(left.heap_redo_page(page_id), right.heap_redo_page(page_id));
    assert_eq!(left.applied_count, right.applied_count);
    assert!(!left.has_errors());
    assert!(!right.has_errors());
}
