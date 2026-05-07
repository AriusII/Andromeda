use andromeda_core::{AndromedaErrorKind, TransactionId};
use andromeda_storage::{Lsn, PageId, PageSize, ProductStockRow};

use super::super::types::{InventoryStock, ReserveStockCommand};
use super::{
    HeapInventoryProductStockStore, InventoryProductStockCommitEvidence, InventoryProductStockStore,
};

fn stock() -> InventoryStock {
    InventoryStock {
        product_id: 42,
        available_quantity: 10,
        version: 7,
    }
}

fn command() -> ReserveStockCommand {
    ReserveStockCommand {
        product_id: 42,
        quantity: 3,
    }
}

#[test]
fn heap_store_keeps_reservation_invisible_until_durable_commit_evidence() {
    let mut store = HeapInventoryProductStockStore::from_cold_snapshot(
        PageId::new(31_001),
        PageSize::KiB16,
        stock(),
    )
    .unwrap();

    let intent = store.prepare_reserve_stock(command()).unwrap();

    assert_eq!(store.visible_stock().unwrap(), stock());
    assert_eq!(
        store.visible_product_stock_row().unwrap(),
        ProductStockRow::new(42, 10).unwrap()
    );
    assert_eq!(store.active_heap_slot_count(), 2);
    assert_eq!(store.prepared_intent(), Some(&intent));
    let prepared_insert = store.prepared_heap_insert().unwrap();
    assert_eq!(prepared_insert.slot_id(), 1);
    assert_eq!(prepared_insert.row(), ProductStockRow::new(42, 7).unwrap());
    assert!(store.last_committed_insert().is_none());
    assert!(store.last_redo_payload().is_none());
    let redo_template = store.prepared_reserve_stock_redo_template(&intent).unwrap();
    let wal_redo = redo_template
        .materialize_heap_redo_payload(Lsn::new(8))
        .unwrap();
    assert_eq!(wal_redo.after_slot_id(), prepared_insert.slot_id());
    assert_eq!(wal_redo.expected_previous_page_lsn(), Lsn::ZERO);
    assert_eq!(wal_redo.resulting_page_lsn(), Lsn::new(8));
    assert_eq!(
        ProductStockRow::decode(wal_redo.tuple()).unwrap(),
        prepared_insert.row()
    );

    let commit =
        InventoryProductStockCommitEvidence::new(TransactionId::new(11), Lsn::new(9)).unwrap();
    let redo = super::InventoryProductStockDurableRedoEvidence::new(
        TransactionId::new(11),
        Lsn::new(9),
        wal_redo.clone(),
    )
    .unwrap();
    store
        .publish_committed_reserve_stock_with_redo(&intent, commit, redo)
        .unwrap();

    assert_eq!(store.visible_stock().unwrap(), intent.effect.next_stock);
    assert_eq!(
        store.visible_product_stock_row().unwrap(),
        ProductStockRow::new(42, 7).unwrap()
    );
    assert_eq!(store.active_heap_slot_count(), 2);
    assert!(store.prepared_intent().is_none());
    assert_eq!(store.published_commit(), Some(commit));
    assert_eq!(store.page_lsn(), Lsn::new(8));

    let insert = store.last_committed_insert().unwrap();
    assert_eq!(insert.slot_id(), 1);
    assert_eq!(insert.row(), ProductStockRow::new(42, 7).unwrap());

    let redo = store.last_redo_payload().unwrap();
    assert_eq!(redo.expected_previous_page_lsn(), Lsn::ZERO);
    assert_eq!(redo.resulting_page_lsn(), Lsn::new(8));
    assert_eq!(redo.after_slot_id(), insert.slot_id());
    assert_eq!(ProductStockRow::decode(redo.tuple()).unwrap(), insert.row());
}

#[test]
fn heap_store_abort_discards_prepared_state_without_heap_publication() {
    let mut store = HeapInventoryProductStockStore::from_cold_snapshot(
        PageId::new(31_002),
        PageSize::KiB16,
        stock(),
    )
    .unwrap();

    let intent = store.prepare_reserve_stock(command()).unwrap();

    store
        .abort_prepared_reserve_stock(&intent, "pre-commit WAL failure")
        .unwrap();

    assert_eq!(store.visible_stock().unwrap(), stock());
    assert_eq!(
        store.visible_product_stock_row().unwrap(),
        ProductStockRow::new(42, 10).unwrap()
    );
    assert_eq!(store.active_heap_slot_count(), 1);
    assert!(store.prepared_intent().is_none());
    assert!(store.prepared_heap_insert().is_none());
    assert!(store.published_commit().is_none());
    assert!(store.last_committed_insert().is_none());
    assert!(store.last_redo_payload().is_none());
    assert_eq!(store.page_lsn(), Lsn::ZERO);
}

#[test]
fn heap_store_rejects_non_advancing_durable_lsn_without_visibility_change() {
    let mut store = HeapInventoryProductStockStore::from_cold_snapshot(
        PageId::new(31_003),
        PageSize::KiB16,
        stock(),
    )
    .unwrap();

    let first = store.prepare_reserve_stock(command()).unwrap();
    let first_template = store.prepared_reserve_stock_redo_template(&first).unwrap();
    let first_redo_payload = first_template
        .materialize_heap_redo_payload(Lsn::new(9))
        .unwrap();
    let first_commit =
        InventoryProductStockCommitEvidence::new(TransactionId::new(11), Lsn::new(10)).unwrap();
    let first_redo = super::InventoryProductStockDurableRedoEvidence::new(
        TransactionId::new(11),
        Lsn::new(10),
        first_redo_payload,
    )
    .unwrap();
    store
        .publish_committed_reserve_stock_with_redo(&first, first_commit, first_redo)
        .unwrap();

    let second = store
        .prepare_reserve_stock(ReserveStockCommand {
            product_id: 42,
            quantity: 2,
        })
        .unwrap();
    let second_template = store.prepared_reserve_stock_redo_template(&second).unwrap();
    assert_eq!(second_template.expected_previous_page_lsn, Lsn::new(9));
    let second_insert = store.prepared_heap_insert().unwrap();
    let second_redo_payload = second_insert
        .row_insert_redo_payload(Lsn::ZERO, Lsn::new(9))
        .unwrap();
    let second_commit =
        InventoryProductStockCommitEvidence::new(TransactionId::new(12), Lsn::new(11)).unwrap();
    let second_redo = super::InventoryProductStockDurableRedoEvidence::new(
        TransactionId::new(12),
        Lsn::new(11),
        second_redo_payload,
    )
    .unwrap();
    let err = store
        .publish_committed_reserve_stock_with_redo(&second, second_commit, second_redo)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(err.message().contains("advance the page LSN"));
    assert_eq!(store.visible_stock().unwrap(), first.effect.next_stock);
    assert_eq!(store.active_heap_slot_count(), 3);
    assert_eq!(store.prepared_intent(), Some(&second));
    assert!(store.prepared_heap_insert().is_some());
    assert_eq!(store.published_commit(), Some(first_commit));
    assert_eq!(store.page_lsn(), Lsn::new(9));
}
