//! HREDOV1 heap row redo replay contracts.
//!
//! These tests cover the recovery-local heap apply target. They deliberately do
//! not claim durable page-store writeback; that bridge remains a later boundary.

use andromeda_core::{AndromedaErrorKind, TransactionId};
use andromeda_storage::{
    DatabaseManifest, InMemoryWal, Lsn, PageId, PageSize, RecoveryPlan, ReplayContext,
    ReplayOutcome, StartupMode, WalRecord, WalRecordKind, execute_redo_plan_into_context,
    replay_wal_record, write_ahead_log::HeapRowRedoPayloadV1,
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
    assert!(err.message().contains("explicit heap page base state"));
    assert!(err.message().contains("snapshot-hydrated"));
    assert_eq!(ctx.error_records.len(), 1);
    assert_eq!(ctx.heap_redo_page_count(), 0);
    assert_eq!(ctx.heap_redo_page_origin(page_id), None);
}

#[test]
fn row_update_unknown_old_slot_fails_without_creating_heap_redo_page() {
    let page_id = PageId::new(711);
    let tx = TransactionId::new(23);
    let update = record(
        WalRecordKind::RowUpdate,
        Lsn::new(41),
        tx,
        heap_redo_payload(
            HeapOp::Update,
            page_id,
            0,
            1,
            Lsn::ZERO,
            Lsn::new(41),
            b"new",
        ),
    );
    let mut ctx = ReplayContext::new();

    let err =
        replay_wal_record(&mut ctx, &update).expect_err("update without prior page must fail stop");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(err.message().contains("explicit heap page base state"));
    assert!(err.message().contains("snapshot-hydrated"));
    assert_eq!(ctx.error_records.len(), 1);
    assert_eq!(ctx.heap_redo_page_count(), 0);
    assert_eq!(ctx.heap_redo_page_origin(page_id), None);
}

#[test]
fn row_delete_on_snapshot_hydrated_page_marks_slot_deleted_without_ram_truth() {
    let page_id = PageId::new(712);
    let tx = TransactionId::new(24);
    let mut ctx = ReplayContext::new();
    ctx.hydrate_heap_redo_page_from_snapshot(
        page_id,
        PageSize::KiB16,
        Lsn::new(90),
        [(3, b"snapshot-victim".to_vec())],
    )
    .expect("cold snapshot hydration evidence");
    let delete = record(
        WalRecordKind::RowDelete,
        Lsn::new(91),
        tx,
        heap_redo_payload(
            HeapOp::Delete,
            page_id,
            3,
            NONE_SLOT_ID,
            Lsn::new(90),
            Lsn::new(91),
            b"",
        ),
    );

    replay_wal_record(&mut ctx, &delete).expect("delete after snapshot hydration");

    assert_eq!(
        ctx.heap_redo_page_origin(page_id),
        Some("snapshot-hydrated")
    );
    let page = ctx.heap_redo_page(page_id).expect("hydrated page remains");
    assert_eq!(page.origin_label(), "snapshot-hydrated");
    assert_eq!(page.page_lsn(), Lsn::new(91));
    assert!(page.is_slot_deleted(3));
    assert_eq!(page.read_tuple(3), None);
    assert!(!ctx.has_errors());
}

#[test]
fn row_update_on_snapshot_hydrated_page_closes_old_slot_and_inserts_new_version() {
    let page_id = PageId::new(713);
    let tx = TransactionId::new(25);
    let mut ctx = ReplayContext::new();
    ctx.hydrate_heap_redo_page_from_snapshot(
        page_id,
        PageSize::KiB16,
        Lsn::new(100),
        [(0, b"snapshot-old".to_vec())],
    )
    .expect("cold snapshot hydration evidence");
    let update = record(
        WalRecordKind::RowUpdate,
        Lsn::new(101),
        tx,
        heap_redo_payload(
            HeapOp::Update,
            page_id,
            0,
            1,
            Lsn::new(100),
            Lsn::new(101),
            b"snapshot-new",
        ),
    );

    replay_wal_record(&mut ctx, &update).expect("update after snapshot hydration");

    assert_eq!(
        ctx.heap_redo_page_origin(page_id),
        Some("snapshot-hydrated")
    );
    let page = ctx.heap_redo_page(page_id).expect("hydrated page remains");
    assert_eq!(page.origin_label(), "snapshot-hydrated");
    assert_eq!(page.page_lsn(), Lsn::new(101));
    assert!(page.is_slot_deleted(0));
    assert_eq!(page.read_tuple(0), None);
    assert_eq!(page.read_tuple(1), Some(b"snapshot-new".as_slice()));
    assert!(!ctx.has_errors());
}

#[test]
fn snapshot_hydration_must_precede_heap_redo_touching_page() {
    let page_id = PageId::new(714);
    let tx = TransactionId::new(26);
    let insert = record(
        WalRecordKind::RowInsert,
        Lsn::new(110),
        tx,
        heap_redo_payload(
            HeapOp::Insert,
            page_id,
            NONE_SLOT_ID,
            0,
            Lsn::ZERO,
            Lsn::new(110),
            b"redo-created",
        ),
    );
    let mut ctx = ReplayContext::new();
    replay_wal_record(&mut ctx, &insert).expect("insert creates redo page state");

    let error = ctx
        .hydrate_heap_redo_page_from_snapshot(
            page_id,
            PageSize::KiB16,
            Lsn::new(109),
            [(0, b"snapshot".to_vec())],
        )
        .expect_err("snapshot hydration after redo must be rejected");

    assert!(
        error
            .message()
            .contains("snapshot hydration must happen before WAL replay touches the page")
    );
    assert_eq!(ctx.heap_redo_page_origin(page_id), Some("redo-created"));
    assert_eq!(
        ctx.heap_redo_page(page_id)
            .expect("redo page remains")
            .read_tuple(0),
        Some(b"redo-created".as_slice())
    );
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

fn append_hredov1_insert(
    wal: &mut InMemoryWal,
    tx: TransactionId,
    page_id: PageId,
    slot_id: u16,
    expected_previous_page_lsn: Lsn,
    tuple: &[u8],
) -> Lsn {
    let resulting_lsn = wal.next_lsn();
    let payload = HeapRowRedoPayloadV1::row_insert(
        page_id,
        PageSize::KiB16,
        slot_id,
        expected_previous_page_lsn,
        resulting_lsn,
        tuple.to_vec(),
    )
    .expect("HREDOV1 insert payload");
    wal.append_payload(payload.wal_record_kind(), Some(tx), payload.encode())
        .expect("append HREDOV1 insert")
}

fn append_hredov1_update(
    wal: &mut InMemoryWal,
    tx: TransactionId,
    page_id: PageId,
    before_slot_id: u16,
    after_slot_id: u16,
    expected_previous_page_lsn: Lsn,
    tuple: &[u8],
) -> Lsn {
    let resulting_lsn = wal.next_lsn();
    let payload = HeapRowRedoPayloadV1::row_update(
        page_id,
        PageSize::KiB16,
        before_slot_id,
        after_slot_id,
        expected_previous_page_lsn,
        resulting_lsn,
        tuple.to_vec(),
    )
    .expect("HREDOV1 update payload");
    wal.append_payload(payload.wal_record_kind(), Some(tx), payload.encode())
        .expect("append HREDOV1 update")
}

fn append_hredov1_delete(
    wal: &mut InMemoryWal,
    tx: TransactionId,
    page_id: PageId,
    slot_id: u16,
    expected_previous_page_lsn: Lsn,
) -> Lsn {
    let resulting_lsn = wal.next_lsn();
    let payload = HeapRowRedoPayloadV1::row_delete(
        page_id,
        PageSize::KiB16,
        slot_id,
        expected_previous_page_lsn,
        resulting_lsn,
    )
    .expect("HREDOV1 delete payload");
    wal.append_payload(payload.wal_record_kind(), Some(tx), payload.encode())
        .expect("append HREDOV1 delete")
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
