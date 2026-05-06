//! HREDOV1 heap row redo replay contracts.
//!
//! These tests cover the recovery-local heap apply target. They deliberately do
//! not claim durable page-store writeback; that bridge remains a later boundary.

use andromeda_core::{AndromedaErrorKind, TransactionId};
use andromeda_storage::{
    Lsn, PageId, PageSize, ReplayContext, ReplayOutcome, WalRecord, WalRecordKind,
    replay_wal_record,
};

const HREDOV1_MAGIC: &[u8; 8] = b"HREDOV1\0";
const NONE_SLOT_ID: u16 = u16::MAX;

#[test]
fn row_insert_hredov1_applies_tuple_and_is_idempotent() {
    let page_id = PageId::new(700);
    let tx = TransactionId::new(10);
    let record = record(
        WalRecordKind::RowInsert,
        Lsn::new(10),
        tx,
        heap_redo_payload(
            HeapOp::Insert,
            page_id,
            NONE_SLOT_ID,
            2,
            Lsn::ZERO,
            Lsn::new(10),
            b"alpha",
        ),
    );
    let mut ctx = ReplayContext::new();

    replay_wal_record(&mut ctx, &record).expect("first insert replay");
    replay_wal_record(&mut ctx, &record).expect("duplicate insert replay must be idempotent");

    assert_eq!(ctx.applied_count, 2);
    assert_eq!(ctx.heap_redo_page_count(), 1);
    assert!(!ctx.has_errors());

    let page = ctx
        .heap_redo_page(page_id)
        .expect("heap redo page should exist");
    assert_eq!(page.page_lsn(), Lsn::new(10));
    assert_eq!(page.slot_count(), 1);
    assert_eq!(page.live_slot_count(), 1);
    assert_eq!(page.read_tuple(2), Some(b"alpha".as_slice()));
}

#[test]
fn row_insert_conflicting_same_slot_tuple_fails_stop() {
    let page_id = PageId::new(701);
    let tx = TransactionId::new(11);
    let insert = record(
        WalRecordKind::RowInsert,
        Lsn::new(20),
        tx,
        heap_redo_payload(
            HeapOp::Insert,
            page_id,
            NONE_SLOT_ID,
            0,
            Lsn::ZERO,
            Lsn::new(20),
            b"alpha",
        ),
    );
    let conflicting_insert = record(
        WalRecordKind::RowInsert,
        Lsn::new(21),
        tx,
        heap_redo_payload(
            HeapOp::Insert,
            page_id,
            NONE_SLOT_ID,
            0,
            Lsn::new(20),
            Lsn::new(21),
            b"beta",
        ),
    );
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
    let insert = record(
        WalRecordKind::RowInsert,
        Lsn::new(30),
        tx,
        heap_redo_payload(
            HeapOp::Insert,
            page_id,
            NONE_SLOT_ID,
            4,
            Lsn::ZERO,
            Lsn::new(30),
            b"victim",
        ),
    );
    let delete = record(
        WalRecordKind::RowDelete,
        Lsn::new(31),
        tx,
        heap_redo_payload(
            HeapOp::Delete,
            page_id,
            4,
            NONE_SLOT_ID,
            Lsn::new(30),
            Lsn::new(31),
            b"",
        ),
    );
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
    let delete = record(
        WalRecordKind::RowDelete,
        Lsn::new(40),
        tx,
        heap_redo_payload(
            HeapOp::Delete,
            page_id,
            7,
            NONE_SLOT_ID,
            Lsn::ZERO,
            Lsn::new(40),
            b"",
        ),
    );
    let mut ctx = ReplayContext::new();

    let err =
        replay_wal_record(&mut ctx, &delete).expect_err("delete of unknown slot must fail stop");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(err.message().contains("unknown"));
    assert_eq!(ctx.error_records.len(), 1);
}

#[test]
fn row_update_closes_old_slot_and_inserts_new_version_idempotently() {
    let page_id = PageId::new(704);
    let tx = TransactionId::new(14);
    let insert = record(
        WalRecordKind::RowInsert,
        Lsn::new(50),
        tx,
        heap_redo_payload(
            HeapOp::Insert,
            page_id,
            NONE_SLOT_ID,
            0,
            Lsn::ZERO,
            Lsn::new(50),
            b"old",
        ),
    );
    let update = record(
        WalRecordKind::RowUpdate,
        Lsn::new(51),
        tx,
        heap_redo_payload(
            HeapOp::Update,
            page_id,
            0,
            1,
            Lsn::new(50),
            Lsn::new(51),
            b"new",
        ),
    );
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
        &record(
            WalRecordKind::RowInsert,
            Lsn::new(60),
            tx,
            heap_redo_payload(
                HeapOp::Insert,
                page_id,
                NONE_SLOT_ID,
                0,
                Lsn::ZERO,
                Lsn::new(60),
                b"old",
            ),
        ),
    )
    .expect("old insert replay");
    replay_wal_record(
        &mut ctx,
        &record(
            WalRecordKind::RowInsert,
            Lsn::new(61),
            tx,
            heap_redo_payload(
                HeapOp::Insert,
                page_id,
                NONE_SLOT_ID,
                1,
                Lsn::new(60),
                Lsn::new(61),
                b"occupied",
            ),
        ),
    )
    .expect("occupied insert replay");

    let update = record(
        WalRecordKind::RowUpdate,
        Lsn::new(62),
        tx,
        heap_redo_payload(
            HeapOp::Update,
            page_id,
            0,
            1,
            Lsn::new(61),
            Lsn::new(62),
            b"new",
        ),
    );
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
        record(
            WalRecordKind::RowInsert,
            Lsn::new(80),
            tx,
            heap_redo_payload(
                HeapOp::Insert,
                page_id,
                NONE_SLOT_ID,
                0,
                Lsn::ZERO,
                Lsn::new(80),
                b"a",
            ),
        ),
        record(
            WalRecordKind::RowUpdate,
            Lsn::new(81),
            tx,
            heap_redo_payload(
                HeapOp::Update,
                page_id,
                0,
                1,
                Lsn::new(80),
                Lsn::new(81),
                b"b",
            ),
        ),
        record(
            WalRecordKind::RowDelete,
            Lsn::new(82),
            tx,
            heap_redo_payload(
                HeapOp::Delete,
                page_id,
                1,
                NONE_SLOT_ID,
                Lsn::new(81),
                Lsn::new(82),
                b"",
            ),
        ),
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

#[derive(Debug, Clone, Copy)]
enum HeapOp {
    Insert,
    Delete,
    Update,
}

fn record(kind: WalRecordKind, lsn: Lsn, tx: TransactionId, payload: Vec<u8>) -> WalRecord {
    WalRecord::from_parts(kind, lsn, None, Some(tx), payload)
        .expect("test WAL record should be structurally valid")
}

fn heap_redo_payload(
    op: HeapOp,
    page_id: PageId,
    before_slot_id: u16,
    after_slot_id: u16,
    expected_previous_page_lsn: Lsn,
    resulting_page_lsn: Lsn,
    tuple: &[u8],
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(44 + tuple.len());
    payload.extend_from_slice(HREDOV1_MAGIC);
    payload.extend_from_slice(&1u16.to_le_bytes());
    payload.push(match op {
        HeapOp::Insert => 1,
        HeapOp::Delete => 2,
        HeapOp::Update => 3,
    });
    payload.push(match PageSize::KiB16 {
        PageSize::KiB16 => 1,
        PageSize::KiB32 => 2,
    });
    payload.extend_from_slice(&page_id.get().to_le_bytes());
    payload.extend_from_slice(&before_slot_id.to_le_bytes());
    payload.extend_from_slice(&after_slot_id.to_le_bytes());
    payload.extend_from_slice(&expected_previous_page_lsn.get().to_le_bytes());
    payload.extend_from_slice(&resulting_page_lsn.get().to_le_bytes());
    payload.extend_from_slice(&(tuple.len() as u32).to_le_bytes());
    payload.extend_from_slice(tuple);
    payload
}
