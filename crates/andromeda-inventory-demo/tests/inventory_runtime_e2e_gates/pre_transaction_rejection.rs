use crate::support::*;

#[test]
fn stale_contract_rejection_stays_pre_transaction_with_no_wal_or_transaction_observability() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let effect = InventoryReserveStockExecutor::reserve(
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
    let procedure = effect.to_local_procedure(&contract).unwrap();

    let stale_request = InvocationRequest {
        expected_contract_hash: ContractHash::test_vector(99),
        catalog_version: CatalogVersion::new(99),
        expected_binding: Some(contract.binding()),
        ..request_for(&contract, 9300)
    };
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let error = runtime
        .execute_internal_authorized(
            stale_request.clone(),
            &procedure,
            &InvocationContext::new(
                TraceId::new(9300),
                vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
            ),
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(runtime.wal().is_empty());

    let reject = stale_request
        .validate_before_transaction(procedure.contract_binding, TraceId::new(9301))
        .unwrap_err();
    assert_eq!(reject.status, CompletionStatus::ContractRejected);
    assert!(
        reject.reason.contains("ContractHash") || reject.reason.contains("CatalogVersion"),
        "stale request rejection should identify contract/catalog mismatch: {}",
        reject.reason
    );

    let correlation = EventCorrelation {
        transaction_id: None,
        durable_lsn: None,
        ..event_correlation(&contract, None, None)
    };
    assert!(correlation.has_no_transaction_evidence());
    let envelope = EventEnvelope::new(
        EventId::new(9301),
        correlation,
        TraceEvent::ContractRejected(
            reject
                .contract_rejected_trace(TraceId::new(9301), ProtocolCorrelation::empty(), 1, 99)
                .unwrap(),
        ),
    )
    .unwrap();

    assert_eq!(
        envelope.event.kind(),
        CriticalDecisionKind::ContractRejected
    );
    assert!(envelope.correlation.has_no_transaction_evidence());
}
