use crate::common::*;

#[test]
fn local_vertical_happy_path_commits_only_with_durable_wal_evidence() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let request = InvocationRequest {
        invocation_id: InvocationId::new(700),
        procedure: contract.as_ref(),
        expected_binding: Some(contract.binding()),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };
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
    let procedure = effect.to_local_procedure(&contract).unwrap();
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let outcome = runtime
        .execute_authorized(
            request,
            &procedure,
            &InvocationContext::new(TraceId::new(7000), contract.required_permissions.clone()),
        )
        .unwrap();

    assert_eq!(outcome.completion.status(), CompletionStatus::Committed);
    assert_eq!(
        outcome.completion.transaction_state(),
        Some(TransactionState::Committed)
    );
    assert_eq!(
        outcome.completion.durable_lsn(),
        Some(runtime.wal().durable_lsn())
    );
    assert_eq!(runtime.wal().durable_lsn(), Lsn::new(3));
    assert_eq!(runtime.wal().replay_durable().len(), 3);
    assert_eq!(outcome.completion.rows_affected(), Some(2));
    assert!(
        effect
            .result_evidence()
            .matches_committed_completion(&outcome.completion)
    );
    assert_eq!(
        runtime.wal().records()[2].header.kind,
        WalRecordKind::TxCommit
    );
}

#[test]
fn inventory_mvcc_store_exposes_only_committed_business_stock_and_reservations() {
    let mut store = InventoryBusinessMvccStore::new();
    let seed_tx = TransactionId::new(100);
    store
        .seed_committed_stock(
            InventoryStock {
                product_id: 42,
                available_quantity: 10,
                version: 1,
            },
            10,
            seed_tx,
        )
        .unwrap();

    let writer = TransactionId::new(101);
    let reader = TransactionId::new(102);
    let writer_snapshot = tx_snapshot(20, writer, [writer]);
    let decision = store
        .reserve_stock(
            writer,
            21,
            &writer_snapshot,
            ReserveStockCommand {
                product_id: 42,
                quantity: 3,
            },
        )
        .unwrap();
    assert!(
        decision
            .evidence
            .proves_reserve_stock_write(&decision.effect)
    );
    assert_eq!(decision.effect.next_stock.available_quantity, 7);

    let reader_snapshot_while_writer_active = tx_snapshot(22, reader, [writer]);
    let visible_before_commit = store
        .read_stock(42, &reader_snapshot_while_writer_active)
        .unwrap()
        .unwrap();
    assert_eq!(visible_before_commit.observed_quantity, 10);
    assert!(
        store
            .read_reservations(42, &reader_snapshot_while_writer_active)
            .unwrap()
            .is_empty()
    );

    store
        .commit_transaction_after_durable_wal(writer, 3)
        .unwrap();
    let reader_snapshot_after_commit = tx_snapshot(30, reader, []);
    let visible_after_commit = store
        .read_stock(42, &reader_snapshot_after_commit)
        .unwrap()
        .unwrap();
    assert_eq!(visible_after_commit.observed_quantity, 7);
    assert_eq!(visible_after_commit.stock_version, 2);
    let reservations = store
        .read_reservations(42, &reader_snapshot_after_commit)
        .unwrap();
    assert_eq!(reservations, vec![decision.reservation]);
}

#[test]
fn inventory_mvcc_store_hides_rolled_back_and_incomplete_business_effects() {
    let mut store = InventoryBusinessMvccStore::new();
    store
        .seed_committed_stock(
            InventoryStock {
                product_id: 42,
                available_quantity: 10,
                version: 1,
            },
            10,
            TransactionId::new(110),
        )
        .unwrap();

    let writer = TransactionId::new(111);
    let writer_snapshot = tx_snapshot(20, writer, [writer]);
    store
        .reserve_stock(
            writer,
            21,
            &writer_snapshot,
            ReserveStockCommand {
                product_id: 42,
                quantity: 4,
            },
        )
        .unwrap();
    store
        .rollback_transaction_after_durable_wal(writer, 2)
        .unwrap();

    let reader_snapshot = tx_snapshot(30, TransactionId::new(112), []);
    let visible = store.read_stock(42, &reader_snapshot).unwrap().unwrap();
    assert_eq!(visible.observed_quantity, 10);
    assert_eq!(visible.stock_version, 1);
    assert!(
        store
            .read_reservations(42, &reader_snapshot)
            .unwrap()
            .is_empty()
    );

    let second_writer = TransactionId::new(113);
    let second_snapshot = tx_snapshot(31, second_writer, [second_writer]);
    let second_decision = store
        .reserve_stock(
            second_writer,
            32,
            &second_snapshot,
            ReserveStockCommand {
                product_id: 42,
                quantity: 2,
            },
        )
        .unwrap();
    assert_eq!(second_decision.effect.previous_stock.available_quantity, 10);
    store
        .commit_transaction_after_durable_wal(second_writer, 4)
        .unwrap();

    let final_visible = store
        .read_stock(42, &tx_snapshot(40, TransactionId::new(114), []))
        .unwrap()
        .unwrap();
    assert_eq!(final_visible.observed_quantity, 8);
}

#[test]
fn inventory_mvcc_store_rejects_stale_or_concurrent_reservations_with_evidence() {
    let mut store = InventoryBusinessMvccStore::new();
    store
        .seed_committed_stock(
            InventoryStock {
                product_id: 42,
                available_quantity: 10,
                version: 1,
            },
            10,
            TransactionId::new(120),
        )
        .unwrap();

    let first_writer = TransactionId::new(121);
    let stale_writer = TransactionId::new(122);
    let stale_snapshot = tx_snapshot(20, stale_writer, [stale_writer]);
    let first_decision = store
        .reserve_stock(
            first_writer,
            21,
            &tx_snapshot(20, first_writer, [first_writer]),
            ReserveStockCommand {
                product_id: 42,
                quantity: 6,
            },
        )
        .unwrap();
    assert_eq!(first_decision.evidence.read_stock.stock_version, 1);

    let concurrent_err = store
        .reserve_stock(
            stale_writer,
            22,
            &stale_snapshot,
            ReserveStockCommand {
                product_id: 42,
                quantity: 1,
            },
        )
        .unwrap_err();
    assert_eq!(
        concurrent_err.kind(),
        andromeda_core::AndromedaErrorKind::Transaction
    );
    assert!(concurrent_err.message().contains("stale"));

    store
        .commit_transaction_after_durable_wal(first_writer, 3)
        .unwrap();
    let stale_err = store
        .reserve_stock(
            stale_writer,
            23,
            &stale_snapshot,
            ReserveStockCommand {
                product_id: 42,
                quantity: 1,
            },
        )
        .unwrap_err();
    assert_eq!(
        stale_err.kind(),
        andromeda_core::AndromedaErrorKind::Transaction
    );
    assert!(stale_err.message().contains("stale"));

    let fresh_writer = TransactionId::new(123);
    let insufficient_err = store
        .reserve_stock(
            fresh_writer,
            31,
            &tx_snapshot(30, fresh_writer, [fresh_writer]),
            ReserveStockCommand {
                product_id: 42,
                quantity: 5,
            },
        )
        .unwrap_err();
    assert_eq!(
        insufficient_err.kind(),
        andromeda_core::AndromedaErrorKind::Execution
    );
    assert!(
        insufficient_err
            .message()
            .contains("insufficient inventory stock")
    );
}

#[test]
fn local_vertical_rejects_visibility_when_flush_does_not_cover_commit_lsn() {
    let mut runtime = LocalVerticalRuntime::new(LaggingFlushWal::default());
    let procedure = procedure(request(ContractHash::test_vector(7)).procedure);

    let err = runtime
        .execute(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(8000),
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Storage);
    assert_eq!(runtime.wal().records.len(), 3);
    assert_eq!(runtime.wal().records[2].1, WalRecordKind::TxCommit);
    assert!(runtime.wal().durable_lsn < runtime.wal().records[2].0);
}

#[test]
fn local_vertical_commit_append_failure_does_not_publish_terminal_status() {
    let mut runtime = LocalVerticalRuntime::new(CommitAppendErrorWal::default());
    let procedure = procedure(request(ContractHash::test_vector(7)).procedure);

    let err = runtime
        .execute(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(8001),
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Storage);
    assert_eq!(runtime.wal().records.len(), 2);
    assert_eq!(runtime.wal().records[0].1, WalRecordKind::TxBegin);
    assert_eq!(runtime.wal().records[1].1, WalRecordKind::RowUpdate);
    assert_eq!(
        runtime
            .transactions()
            .status(TransactionId::new(1))
            .expect("transaction status lookup must succeed"),
        Some(TransactionStatus::InFlight),
        "commit append failure occurs before request_commit/commit_durable, so status must not become Committed"
    );
    assert_eq!(
        runtime
            .transactions()
            .snapshot(TransactionId::new(1))
            .expect("transaction snapshot lookup must succeed")
            .expect("transaction record must exist")
            .state_machine
            .state(),
        TransactionState::Active
    );
}

#[test]
fn local_vertical_flush_error_does_not_publish_terminal_status() {
    let mut runtime = LocalVerticalRuntime::new(FlushErrorWal::default());
    let procedure = procedure(request(ContractHash::test_vector(7)).procedure);

    let err = runtime
        .execute(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(8002),
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Storage);
    assert_eq!(runtime.wal().records.len(), 3);
    assert_eq!(runtime.wal().records[2].1, WalRecordKind::TxCommit);
    assert_eq!(
        runtime
            .transactions()
            .status(TransactionId::new(1))
            .expect("transaction status lookup must succeed"),
        Some(TransactionStatus::InFlight),
        "flush failure occurs before commit_durable, so visible commit status must not be published"
    );
    assert_eq!(
        runtime
            .transactions()
            .snapshot(TransactionId::new(1))
            .expect("transaction snapshot lookup must succeed")
            .expect("transaction record must exist")
            .state_machine
            .state(),
        TransactionState::Active
    );
}
