use crate::common::*;

#[test]
fn local_dispatcher_rolls_back_only_with_durable_wal_evidence() {
    let mut wal = RecordingWal::default();
    let receipt = LocalDispatcher::new(&mut wal)
        .dispatch_rollback(LocalRollbackPlan {
            transaction_id: TransactionId::new(900),
            rollback_payload: b"business-validation-failed".to_vec(),
        })
        .unwrap();

    assert_eq!(receipt.transaction_state, TransactionState::RolledBack);
    assert_eq!(receipt.durable_lsn, Lsn::new(2));
    assert_eq!(receipt.wal_evidence.begin_lsn, Lsn::new(1));
    assert_eq!(receipt.wal_evidence.rollback_lsn, Lsn::new(2));
    assert_eq!(receipt.wal_evidence.durable_lsn, Lsn::new(2));
    assert_eq!(wal.records.len(), 2);
    assert_eq!(wal.records[0].1, WalRecordKind::TxBegin);
    assert_eq!(wal.records[1].1, WalRecordKind::TxRollback);
    assert_eq!(wal.records[1].3.as_slice(), b"business-validation-failed");
    assert_eq!(wal.durable_lsn, wal.records[1].0);
}

#[test]
fn local_dispatcher_rejects_rolled_back_completion_when_flush_lags_rollback_lsn() {
    let mut wal = LaggingFlushWal::default();

    let err = LocalDispatcher::new(&mut wal)
        .dispatch_rollback(LocalRollbackPlan {
            transaction_id: TransactionId::new(901),
            rollback_payload: b"business-validation-failed".to_vec(),
        })
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Storage);
    assert_eq!(wal.records.len(), 2);
    assert_eq!(wal.records[1].1, WalRecordKind::TxRollback);
    assert_eq!(wal.records[1].3.as_slice(), b"business-validation-failed");
    assert!(wal.durable_lsn < wal.records[1].0);
}

#[test]
fn local_vertical_runtime_rolls_back_business_validation_failure_after_begin() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let procedure = procedure(request(ContractHash::test_vector(7)).procedure);

    let outcome = runtime
        .rollback_business_validation_failure_after_begin(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(8100),
            "insufficient inventory stock for reservation",
        )
        .unwrap();

    assert_eq!(outcome.completion.status(), CompletionStatus::RolledBack);
    assert_eq!(
        outcome.completion.transaction_state(),
        Some(TransactionState::RolledBack)
    );
    assert_eq!(outcome.completion.rows_affected(), Some(0));
    assert_eq!(outcome.completion.durable_lsn(), Some(Lsn::new(2)));
    assert_eq!(runtime.wal().records.len(), 2);
    assert_eq!(runtime.wal().records[0].1, WalRecordKind::TxBegin);
    assert_eq!(runtime.wal().records[1].1, WalRecordKind::TxRollback);
    assert_eq!(
        runtime.wal().records[1].3.as_slice(),
        b"andromeda.exec.business-validation-failed.v1\0insufficient inventory stock for reservation"
    );
    assert_eq!(runtime.wal().durable_lsn, runtime.wal().records[1].0);
}

#[test]
fn inventory_reserve_stock_business_failure_rolls_back_after_authorized_begin_without_commit() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let valid_effect = InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: 1,
        },
        InventoryStock {
            product_id: 42,
            available_quantity: 10,
            version: 7,
        },
    )
    .unwrap();
    let procedure = valid_effect.to_local_procedure(&contract).unwrap();

    let rejected_command = ReserveStockCommand {
        product_id: 42,
        quantity: 11,
    };
    let observed_stock = InventoryStock {
        product_id: 42,
        available_quantity: 10,
        version: 7,
    };
    let error =
        InventoryReserveStockExecutor::reserve(rejected_command, observed_stock).unwrap_err();
    assert_eq!(error.kind(), andromeda_core::AndromedaErrorKind::Execution);
    assert!(error.message().contains("insufficient inventory stock"));

    let rejection_evidence = InventoryReserveStockExecutor::rejection_evidence(
        rejected_command,
        observed_stock,
        error.message(),
    );
    assert!(rejection_evidence.has_business_rule_evidence());
    assert!(
        rejection_evidence
            .decision_trace(TraceId::new(8200))
            .has_explanation()
    );

    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let request = InvocationRequest {
        invocation_id: InvocationId::new(820),
        procedure: contract.as_ref(),
        expected_binding: Some(contract.binding()),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };

    let outcome = runtime
        .rollback_authorized_business_validation_failure_after_begin(
            request,
            &procedure,
            &InvocationContext::new(
                TraceId::new(8200),
                vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
            ),
            rejection_evidence.reason.as_str(),
        )
        .unwrap();

    assert_eq!(outcome.completion.status(), CompletionStatus::RolledBack);
    assert_eq!(
        outcome.completion.transaction_state(),
        Some(TransactionState::RolledBack)
    );
    assert_eq!(outcome.completion.rows_affected(), Some(0));
    assert_eq!(
        outcome.completion.durable_lsn(),
        Some(runtime.wal().durable_lsn())
    );
    assert_eq!(runtime.wal().durable_lsn(), Lsn::new(2));
    assert_eq!(runtime.wal().records().len(), 2);
    assert_eq!(
        runtime.wal().records()[0].header.kind,
        WalRecordKind::TxBegin
    );
    assert_eq!(
        runtime.wal().records()[1].header.kind,
        WalRecordKind::TxRollback
    );
    assert_eq!(
        runtime.wal().records()[1].payload.as_slice(),
        b"andromeda.exec.business-validation-failed.v1\0insufficient inventory stock"
    );
    assert!(
        !runtime
            .wal()
            .records()
            .iter()
            .any(|record| record.header.kind == WalRecordKind::TxCommit)
    );
    assert!(outcome.authorization_trace.is_some());
}

#[test]
fn local_vertical_runtime_preserves_contract_check_before_rollback_begin() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let procedure = procedure(request(ContractHash::test_vector(7)).procedure);

    let err = runtime
        .rollback_business_validation_failure_after_begin(
            request(ContractHash::test_vector(8)),
            &procedure,
            TraceId::new(8101),
            "insufficient inventory stock for reservation",
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(runtime.wal().records.is_empty());
}

#[test]
fn poison_rollback_routes_through_poisoned_state_with_durable_evidence() {
    use andromeda_exec::RollbackCause;

    let contract = inventory_reserve_stock_contract().unwrap();
    let valid_effect = InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: 1,
        },
        InventoryStock {
            product_id: 42,
            available_quantity: 10,
            version: 7,
        },
    )
    .unwrap();
    let procedure = valid_effect.to_local_procedure(&contract).unwrap();

    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let request = InvocationRequest {
        invocation_id: InvocationId::new(821),
        procedure: contract.as_ref(),
        expected_binding: Some(contract.binding()),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };

    let outcome = runtime
        .rollback_authorized_poison_after_begin(
            request,
            &procedure,
            &InvocationContext::new(
                TraceId::new(8210),
                vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
            ),
            "post-begin runtime witness contradicted MVCC snapshot",
        )
        .unwrap();

    // Visible completion: RolledBack only after durable rollback evidence.
    assert_eq!(outcome.completion.status(), CompletionStatus::RolledBack);
    assert_eq!(
        outcome.completion.transaction_state(),
        Some(TransactionState::RolledBack)
    );
    assert_eq!(outcome.completion.rows_affected(), Some(0));
    assert_eq!(
        outcome.completion.durable_lsn(),
        Some(runtime.wal().durable_lsn())
    );

    // WAL evidence: TxBegin then TxRollback only (never TxCommit).
    assert_eq!(runtime.wal().records().len(), 2);
    assert_eq!(
        runtime.wal().records()[0].header.kind,
        WalRecordKind::TxBegin
    );
    assert_eq!(
        runtime.wal().records()[1].header.kind,
        WalRecordKind::TxRollback
    );
    assert!(
        !runtime
            .wal()
            .records()
            .iter()
            .any(|record| record.header.kind == WalRecordKind::TxCommit)
    );
    assert_eq!(
        runtime.wal().records()[1].payload.as_slice(),
        b"andromeda.exec.poisoned-rollback.v1\0post-begin runtime witness contradicted MVCC snapshot"
    );
    assert!(outcome.authorization_trace.is_some());

    // Dispatcher level: poison cause must record the Poisoned intermediate
    // state on the receipt so audit/recovery can prove the routing.
    let mut wal = RecordingWal::default();
    let receipt = LocalDispatcher::new(&mut wal)
        .dispatch_rollback_with_cause(
            LocalRollbackPlan {
                transaction_id: TransactionId::new(0xCAFEBABE),
                rollback_payload: b"engine invariant breach".to_vec(),
            },
            RollbackCause::Poison,
        )
        .unwrap();
    assert_eq!(receipt.cause, RollbackCause::Poison);
    assert_eq!(receipt.intermediate_state, Some(TransactionState::Poisoned));
    assert_eq!(receipt.transaction_state, TransactionState::RolledBack);
    assert!(receipt.durable_lsn >= receipt.wal_evidence.rollback_lsn);
    assert_eq!(wal.records[1].3.as_slice(), b"engine invariant breach");
}
