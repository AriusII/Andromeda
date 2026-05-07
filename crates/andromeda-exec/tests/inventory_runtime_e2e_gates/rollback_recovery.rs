use crate::support::*;

#[test]
fn insufficient_stock_rolls_back_with_typed_rejection_and_committed_only_recovery_evidence() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = InventoryReserveStockExecutor::reserve(
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
    .unwrap()
    .to_local_procedure(&contract)
    .unwrap();

    let rejected_command = ReserveStockCommand {
        product_id: 42,
        quantity: 11,
    };
    let observed_stock = InventoryStock {
        product_id: 42,
        available_quantity: 10,
        version: 7,
    };
    let business_error =
        InventoryReserveStockExecutor::reserve(rejected_command, observed_stock).unwrap_err();
    assert_eq!(business_error.kind(), AndromedaErrorKind::Execution);
    assert!(
        business_error
            .message()
            .contains("insufficient inventory stock")
    );

    let rejection_evidence = InventoryReserveStockExecutor::rejection_evidence(
        rejected_command,
        observed_stock,
        business_error.message(),
    );
    assert!(rejection_evidence.has_business_rule_evidence());
    assert!(
        rejection_evidence
            .decision_trace(TraceId::new(9200))
            .reason
            .contains("Insufficient")
            || rejection_evidence
                .decision_trace(TraceId::new(9200))
                .reason
                .contains("insufficient")
    );

    let invocation_id = 9200;
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let outcome = runtime
        .rollback_authorized_business_validation_failure_after_begin(
            request_for(&contract, invocation_id),
            &procedure,
            &InvocationContext::new(
                TraceId::new(9200),
                vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
            ),
            rejection_evidence.reason.as_str(),
        )
        .unwrap();

    let tx_id = outcome.transaction_id;
    assert_eq!(outcome.completion.status(), CompletionStatus::RolledBack);
    assert_eq!(
        outcome.completion.transaction_state(),
        Some(TransactionState::RolledBack)
    );
    assert_eq!(outcome.completion.rows_affected(), Some(0));
    assert_eq!(
        runtime
            .wal()
            .records()
            .iter()
            .map(|record| record.header.kind)
            .collect::<Vec<_>>(),
        vec![WalRecordKind::TxBegin, WalRecordKind::TxRollback]
    );
    assert!(!runtime.wal().records().iter().any(|record| matches!(
        record.header.kind,
        WalRecordKind::RowInsert | WalRecordKind::RowUpdate | WalRecordKind::TxCommit
    )));
    assert_eq!(observed_stock.available_quantity, 10);
    assert_eq!(observed_stock.version, 7);

    let durable_records = runtime.wal().replay_durable();
    let recovery_plan = RecoveryPlan::from_manifest_and_wal(
        &recovery_manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &durable_records,
    )
    .unwrap();
    assert!(recovery_plan.replay_lsns().next().is_none());
    assert!(recovery_plan.committed_redo_records().next().is_none());
    assert_eq!(
        recovery_plan
            .records
            .iter()
            .find(|record| record.lsn == Lsn::new(2))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipNonRedoRecord
    );

    let classifications = classify_durable_transactions(durable_records.iter());
    assert_eq!(
        classifications
            .rolled_back_transaction_ids()
            .collect::<Vec<_>>(),
        vec![tx_id]
    );
    assert!(classifications.committed_transaction_ids().next().is_none());

    let durable_lsn = runtime.wal().durable_lsn();
    let correlation = event_correlation(&contract, Some(tx_id), Some(durable_lsn));
    let rollback_envelope = EventEnvelope::new(
        EventId::new(9201),
        correlation,
        TraceEvent::RollbackDurable(RollbackDurableTrace {
            trace_id: TraceId::new(9201),
            transaction_id: tx_id,
            durable_rollback_lsn: durable_lsn.get(),
        }),
    )
    .unwrap();
    let completion_envelope = EventEnvelope::new(
        EventId::new(9202),
        correlation,
        TraceEvent::CompletionEmitted(CompletionEmittedTrace {
            trace_id: TraceId::new(9202),
            protocol: ProtocolCorrelation::empty(),
            completion_code: Some(2),
            committed: false,
            durable_lsn: Some(durable_lsn.get()),
            reason: "Inventory.ReserveStock rolled back business rejection with durable WAL"
                .to_string(),
        }),
    )
    .unwrap();
    let business_envelope = EventEnvelope::new(
        EventId::new(9203),
        correlation,
        TraceEvent::Decision(rejection_evidence.decision_trace(TraceId::new(9203))),
    )
    .unwrap();

    assert_eq!(
        rollback_envelope.event.kind(),
        CriticalDecisionKind::RollbackDurable
    );
    assert_eq!(
        completion_envelope.event.kind(),
        CriticalDecisionKind::CompletionEmitted
    );
    assert_eq!(
        business_envelope.event.kind(),
        CriticalDecisionKind::BusinessRuleDecision
    );
}
