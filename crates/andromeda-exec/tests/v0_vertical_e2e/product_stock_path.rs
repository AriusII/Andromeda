use crate::support::{
    CountingProductStockStore, assert_product_stock_untouched, context, encoded_execute_frame,
    executable_procedure, inventory_catalog_snapshot, request, stock,
};
use andromeda_catalog::inventory_reserve_stock_contract;
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, ContractHash, TransactionId,
};
use andromeda_exec::{
    HeapInventoryProductStockStore, InvocationWal, V0InventoryRecoverableRuntime,
};
use andromeda_storage::{
    InMemoryWal, Lsn, PageId, PageSize, ProductStockRow, WalRecordKind,
    write_ahead_log::HeapRowRedoPayloadV1,
};

#[test]
fn v0_inventory_heap_product_stock_store_publishes_only_after_durable_commit() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut product_stock = HeapInventoryProductStockStore::from_cold_snapshot(
        PageId::new(42_101),
        PageSize::KiB16,
        stock(),
    )
    .unwrap();

    let outcome = runtime
        .execute_encoded_inventory_reserve_stock_with_product_stock(
            &encoded_execute_frame(),
            &procedure,
            request(&contract, 714),
            &context(&contract, 7014),
            &mut product_stock,
        )
        .unwrap();

    assert_eq!(
        outcome.product_stock_commit.transaction_id,
        outcome.vertical.transaction_id
    );
    assert_eq!(
        outcome.product_stock_commit.durable_commit_lsn,
        outcome.vertical.completion.durable_lsn().unwrap()
    );
    assert_eq!(
        product_stock.visible_stock().unwrap(),
        outcome.effect.next_stock
    );
    assert_eq!(
        product_stock.visible_product_stock_row().unwrap(),
        ProductStockRow::new(42, 7).unwrap()
    );
    assert_eq!(product_stock.active_heap_slot_count(), 2);
    assert!(product_stock.prepared_intent().is_none());
    assert_eq!(
        product_stock.published_commit(),
        Some(outcome.product_stock_commit)
    );
    let redo_evidence = &outcome.product_stock_redo;
    assert_eq!(
        redo_evidence.transaction_id,
        outcome.vertical.transaction_id
    );
    assert_eq!(redo_evidence.redo_record_lsn, Lsn::new(2));
    assert_eq!(
        redo_evidence.durable_commit_lsn,
        outcome.product_stock_commit.durable_commit_lsn
    );
    assert_eq!(product_stock.page_lsn(), redo_evidence.redo_record_lsn);

    let insert = product_stock.last_committed_insert().unwrap();
    assert_eq!(insert.slot_id(), 1);
    assert_eq!(insert.row(), ProductStockRow::new(42, 7).unwrap());

    let redo = product_stock.last_redo_payload().unwrap();
    assert_eq!(redo.expected_previous_page_lsn(), Lsn::ZERO);
    assert_eq!(redo.resulting_page_lsn(), redo_evidence.redo_record_lsn);
    assert_eq!(redo.after_slot_id(), insert.slot_id());
    assert_eq!(ProductStockRow::decode(redo.tuple()).unwrap(), insert.row());
    assert_eq!(redo, &redo_evidence.redo_payload);
    assert_eq!(
        runtime
            .wal()
            .records()
            .iter()
            .map(|record| record.header.kind)
            .collect::<Vec<_>>(),
        vec![
            WalRecordKind::TxBegin,
            WalRecordKind::RowInsert,
            WalRecordKind::TxCommit,
        ]
    );
    let wal_redo = HeapRowRedoPayloadV1::decode(
        &runtime.wal().records()[1].payload,
        runtime.wal().records()[1].header.kind,
    )
    .unwrap();
    assert_eq!(wal_redo, redo.clone());
}

#[test]
fn v0_inventory_product_stock_adapter_aborts_prepared_heap_state_when_commit_evidence_is_absent() {
    #[derive(Debug, Default)]
    struct CommitAppendFailWal {
        records: Vec<(WalRecordKind, Option<TransactionId>, Vec<u8>)>,
    }

    impl InvocationWal for CommitAppendFailWal {
        fn append(
            &mut self,
            kind: WalRecordKind,
            transaction_id: Option<TransactionId>,
            payload: &[u8],
        ) -> AndromedaResult<Lsn> {
            if kind == WalRecordKind::TxCommit {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    "injected commit append failure before ProductStock publication",
                ));
            }

            let lsn = Lsn::new(self.records.len() as u64 + 1);
            self.records.push((kind, transaction_id, payload.to_vec()));
            Ok(lsn)
        }

        fn flush_through(&mut self, _lsn: Lsn) -> AndromedaResult<Lsn> {
            unreachable!("commit append failure must prevent durable commit evidence")
        }
    }

    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut runtime = V0InventoryRecoverableRuntime::new(CommitAppendFailWal::default());
    let mut product_stock = CountingProductStockStore::new(stock());

    let err = runtime
        .execute_encoded_inventory_reserve_stock_with_product_stock(
            &encoded_execute_frame(),
            &procedure,
            request(&contract, 713),
            &context(&contract, 7013),
            &mut product_stock,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(err.message().contains("commit append failure"));
    assert_eq!(product_stock.prepare_count, 1);
    assert_eq!(product_stock.publish_count, 0);
    assert_eq!(product_stock.abort_count, 1);
    assert_eq!(product_stock.inner.visible_stock().unwrap(), stock());
    assert_eq!(product_stock.inner.active_heap_slot_count(), 1);
    assert!(product_stock.inner.prepared_intent().is_none());
    assert!(product_stock.inner.published_commit().is_none());
    assert!(product_stock.inner.last_committed_insert().is_none());
    assert!(product_stock.inner.last_redo_payload().is_none());
    assert_eq!(product_stock.inner.page_lsn(), Lsn::ZERO);
    assert_eq!(
        runtime
            .wal()
            .records
            .iter()
            .map(|(kind, _, _)| *kind)
            .collect::<Vec<_>>(),
        vec![WalRecordKind::TxBegin, WalRecordKind::RowInsert]
    );
}

#[test]
fn v0_inventory_product_stock_adapter_is_not_touched_for_contract_rejection() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut stale_request = request(&contract, 712);
    stale_request.expected_contract_hash = ContractHash::test_vector(0xBA);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut product_stock = CountingProductStockStore::new(stock());

    let err = runtime
        .execute_encoded_inventory_reserve_stock_with_product_stock(
            &encoded_execute_frame(),
            &procedure,
            stale_request,
            &context(&contract, 7012),
            &mut product_stock,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(runtime.wal().is_empty());
    assert_product_stock_untouched(&product_stock);
}
