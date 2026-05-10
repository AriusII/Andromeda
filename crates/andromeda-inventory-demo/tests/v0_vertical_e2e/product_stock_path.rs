use crate::support::{
    CountingProductStockStore, assert_product_stock_untouched, context, encoded_execute_frame,
    executable_procedure, inventory_catalog_snapshot, request, stock,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_inventory_demo::{
    HeapInventoryProductStockStore, InventoryProductStockCommitEvidence,
    InventoryProductStockDurableRedoEvidence, InventoryProductStockReservationIntent,
    InventoryProductStockStore, ReserveStockCommand, V0InventoryRecoverableRuntime,
    inventory_reserve_stock_catalog_bindings, inventory_reserve_stock_contract,
};
use andromeda_procedure_contract::ProcedureContract;
use andromeda_storage_heap::{
    HeapRowRedoPayloadV1, LocalHeapRowRedoContractBinding, ProductStockRow,
};
use andromeda_storage_page::{PageId, PageSize};
use andromeda_types::{ContractHash, TransactionId};
use andromeda_wal::{InMemoryWal, InvocationWal, Lsn, WalRecordKind};

fn reserve_three_units_command() -> ReserveStockCommand {
    ReserveStockCommand {
        product_id: 42,
        quantity: 3,
    }
}

fn fresh_product_stock_store(page_id: u64) -> HeapInventoryProductStockStore {
    HeapInventoryProductStockStore::from_cold_snapshot(
        PageId::new(page_id),
        PageSize::KiB16,
        stock(),
    )
    .unwrap()
}

fn product_stock_redo_binding(contract: &ProcedureContract) -> LocalHeapRowRedoContractBinding {
    let bindings =
        inventory_reserve_stock_catalog_bindings(contract.object.catalog_version).unwrap();
    LocalHeapRowRedoContractBinding::from_procedure_binding(
        bindings.product_stock_table.object_id,
        contract.binding(),
    )
    .unwrap()
}

fn assert_prepared_state_remains_unpublished(
    product_stock: &HeapInventoryProductStockStore,
    prepared: &InventoryProductStockReservationIntent,
) {
    assert_eq!(product_stock.visible_stock().unwrap(), stock());
    assert_eq!(
        product_stock.visible_product_stock_row().unwrap(),
        ProductStockRow::new(42, 10).unwrap()
    );
    assert_eq!(product_stock.prepared_intent(), Some(prepared));
    assert!(product_stock.prepared_heap_insert().is_some());
    assert!(product_stock.published_commit().is_none());
    assert!(product_stock.last_committed_insert().is_none());
    assert!(product_stock.last_redo_payload().is_none());
}

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
    assert!(
        redo_evidence.durable_commit_lsn > redo_evidence.redo_record_lsn,
        "ProductStock publication must prove durable TxCommit follows HREDOV1 RowInsert"
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
fn v0_inventory_product_stock_rejects_durable_commit_lsn_not_after_redo_record() {
    let mut product_stock = fresh_product_stock_store(42_102);
    let prepared = product_stock
        .prepare_reserve_stock(reserve_three_units_command())
        .unwrap();
    let redo_template = product_stock
        .prepared_reserve_stock_redo_template(&prepared)
        .unwrap();
    let redo_payload = redo_template
        .materialize_heap_redo_payload(Lsn::new(2))
        .unwrap();

    let err = InventoryProductStockDurableRedoEvidence::new(
        TransactionId::new(82),
        Lsn::new(2),
        redo_payload,
    )
    .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(
        err.message()
            .contains("durable commit LSN must follow the row redo record LSN")
    );
    assert_prepared_state_remains_unpublished(&product_stock, &prepared);
    assert_eq!(product_stock.page_lsn(), Lsn::ZERO);
}

#[test]
fn v0_inventory_product_stock_rejects_mismatched_redo_binding_without_publication() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let expected_binding = product_stock_redo_binding(&contract);
    let mut product_stock = fresh_product_stock_store(42_103)
        .with_redo_contract_binding(expected_binding)
        .unwrap();
    let prepared = product_stock
        .prepare_reserve_stock(reserve_three_units_command())
        .unwrap();
    let redo_template = product_stock
        .prepared_reserve_stock_redo_template(&prepared)
        .unwrap();
    assert_eq!(redo_template.redo_binding(), Some(expected_binding));
    let redo_payload = redo_template
        .materialize_heap_redo_payload(Lsn::new(2))
        .unwrap();
    let mismatched_binding = LocalHeapRowRedoContractBinding::new(
        expected_binding.table_object_id,
        expected_binding.procedure_id,
        expected_binding.catalog_version,
        ContractHash::test_vector(0xE9),
    )
    .unwrap();
    let redo = InventoryProductStockDurableRedoEvidence::new_with_binding(
        TransactionId::new(83),
        Lsn::new(3),
        mismatched_binding,
        redo_payload,
    )
    .unwrap();
    let commit =
        InventoryProductStockCommitEvidence::new(TransactionId::new(83), Lsn::new(3)).unwrap();

    let err = product_stock
        .publish_committed_reserve_stock_with_redo(&prepared, commit, redo)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("expected table object"));
    assert_prepared_state_remains_unpublished(&product_stock, &prepared);
    assert_eq!(product_stock.page_lsn(), Lsn::ZERO);
}

#[test]
fn v0_inventory_product_stock_rejects_redo_record_lsn_that_does_not_advance_page_lsn() {
    let mut product_stock = fresh_product_stock_store(42_104);
    let first = product_stock
        .prepare_reserve_stock(reserve_three_units_command())
        .unwrap();
    let first_redo_payload = product_stock
        .prepared_reserve_stock_redo_template(&first)
        .unwrap()
        .materialize_heap_redo_payload(Lsn::new(2))
        .unwrap();
    let first_commit =
        InventoryProductStockCommitEvidence::new(TransactionId::new(84), Lsn::new(3)).unwrap();
    let first_redo = InventoryProductStockDurableRedoEvidence::new(
        TransactionId::new(84),
        Lsn::new(3),
        first_redo_payload,
    )
    .unwrap();
    product_stock
        .publish_committed_reserve_stock_with_redo(&first, first_commit, first_redo)
        .unwrap();
    assert_eq!(product_stock.page_lsn(), Lsn::new(2));

    let second = product_stock
        .prepare_reserve_stock(ReserveStockCommand {
            product_id: 42,
            quantity: 2,
        })
        .unwrap();
    let second_insert = product_stock.prepared_heap_insert().unwrap();
    let stale_page_lsn_redo_payload = second_insert
        .row_insert_redo_payload(Lsn::ZERO, Lsn::new(2))
        .unwrap();
    let second_commit =
        InventoryProductStockCommitEvidence::new(TransactionId::new(85), Lsn::new(4)).unwrap();
    let second_redo = InventoryProductStockDurableRedoEvidence::new(
        TransactionId::new(85),
        Lsn::new(4),
        stale_page_lsn_redo_payload,
    )
    .unwrap();

    let err = product_stock
        .publish_committed_reserve_stock_with_redo(&second, second_commit, second_redo)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(err.message().contains("advance the page LSN"));
    assert_eq!(
        product_stock.visible_stock().unwrap(),
        first.effect.next_stock
    );
    assert_eq!(product_stock.prepared_intent(), Some(&second));
    assert!(product_stock.prepared_heap_insert().is_some());
    assert_eq!(product_stock.published_commit(), Some(first_commit));
    assert_eq!(product_stock.page_lsn(), Lsn::new(2));
    assert_eq!(
        product_stock
            .last_redo_payload()
            .unwrap()
            .resulting_page_lsn(),
        Lsn::new(2)
    );
}

#[test]
fn v0_inventory_product_stock_rejects_redo_payload_that_does_not_match_prepared_insert() {
    let mut product_stock = fresh_product_stock_store(42_105);
    let prepared = product_stock
        .prepare_reserve_stock(reserve_three_units_command())
        .unwrap();
    let mut mismatched_redo_payload = product_stock
        .prepared_reserve_stock_redo_template(&prepared)
        .unwrap()
        .materialize_heap_redo_payload(Lsn::new(2))
        .unwrap();
    let tuple_tail = mismatched_redo_payload
        .tuple
        .last_mut()
        .expect("prepared ProductStock HREDOV1 tuple is non-empty");
    *tuple_tail ^= 1;
    let commit =
        InventoryProductStockCommitEvidence::new(TransactionId::new(86), Lsn::new(3)).unwrap();
    let redo = InventoryProductStockDurableRedoEvidence::new(
        TransactionId::new(86),
        Lsn::new(3),
        mismatched_redo_payload,
    )
    .unwrap();

    let err = product_stock
        .publish_committed_reserve_stock_with_redo(&prepared, commit, redo)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(
        err.message()
            .contains("durable redo payload does not match prepared heap insert")
    );
    assert_prepared_state_remains_unpublished(&product_stock, &prepared);
    assert_eq!(product_stock.page_lsn(), Lsn::ZERO);
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
