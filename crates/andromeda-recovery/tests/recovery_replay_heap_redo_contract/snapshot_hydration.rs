use crate::support::{row_delete_record, row_insert_record, row_update_record};
use andromeda_recovery::{ReplayContext, replay_wal_record};
use andromeda_storage_page::{PageId, PageSize};
use andromeda_types::TransactionId;
use andromeda_wal::Lsn;

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
    let delete = row_delete_record(Lsn::new(91), tx, page_id, 3, Lsn::new(90));

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
    let update = row_update_record(
        Lsn::new(101),
        tx,
        page_id,
        0,
        1,
        Lsn::new(100),
        b"snapshot-new",
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
    let insert = row_insert_record(Lsn::new(110), tx, page_id, 0, Lsn::ZERO, b"redo-created");
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
