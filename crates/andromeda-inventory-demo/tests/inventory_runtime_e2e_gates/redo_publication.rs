use crate::support::*;

#[test]
fn product_stock_redo_publication_rejects_mismatched_contract_binding() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let redo_binding = product_stock_redo_binding(&contract);
    let effect = InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: 3,
        },
        InventoryStock {
            product_id: 42,
            available_quantity: 10,
            version: 7,
        },
    )
    .unwrap();
    let mut product_stock = HeapInventoryProductStockStore::from_cold_snapshot(
        PageId::new(42_202),
        PageSize::KiB16,
        effect.previous_stock,
    )
    .unwrap()
    .with_redo_contract_binding(redo_binding)
    .unwrap();
    let prepared_product_stock = product_stock
        .prepare_reserve_stock(ReserveStockCommand {
            product_id: 42,
            quantity: 3,
        })
        .unwrap();
    let redo_template = product_stock
        .prepared_reserve_stock_redo_template(&prepared_product_stock)
        .unwrap();
    let redo_payload = redo_template
        .materialize_heap_redo_payload(Lsn::new(2))
        .unwrap();
    let mismatched_binding = LocalHeapRowRedoContractBinding::new(
        redo_binding.table_object_id,
        redo_binding.procedure_id,
        redo_binding.catalog_version,
        ContractHash::test_vector(0xEE),
    )
    .unwrap();
    let redo = InventoryProductStockDurableRedoEvidence::new_with_binding(
        TransactionId::new(77),
        Lsn::new(3),
        mismatched_binding,
        redo_payload,
    )
    .unwrap();
    let commit =
        InventoryProductStockCommitEvidence::new(TransactionId::new(77), Lsn::new(3)).unwrap();

    let error = product_stock
        .publish_committed_reserve_stock_with_redo(&prepared_product_stock, commit, redo)
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("expected table object"));
    assert_eq!(
        product_stock.visible_stock().unwrap(),
        effect.previous_stock
    );
    assert_eq!(
        product_stock.visible_product_stock_row().unwrap(),
        ProductStockRow::new(42, 10).unwrap()
    );
    assert!(product_stock.published_commit().is_none());
    assert_eq!(
        product_stock.prepared_intent(),
        Some(&prepared_product_stock)
    );
}
