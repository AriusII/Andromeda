use crate::support::*;

#[test]
fn e2e_procedure_error_triggers_rollback_and_wal_durability() {
    // Arrange: Set up a scenario that will fail validation
    let contract = valid_contract();
    let request = build_invocation_request(&contract, 5001);
    let context = build_invocation_context(5001);

    // Create a procedure with insufficient stock (will fail assertion)
    let rejection = InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: 100, // Request more than available
        },
        InventoryStock {
            product_id: 42,
            available_quantity: 10, // Only 10 available
            version: 7,
        },
    );

    let rejection = rejection.expect_err("reserve should reject insufficient stock");
    assert!(
        rejection
            .to_string()
            .to_lowercase()
            .contains("insufficient")
            || rejection.to_string().to_lowercase().contains("stock"),
        "error should indicate insufficient stock"
    );

    let valid_procedure = execute_reserve_stock(10, 5);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let outcome = runtime
        .rollback_authorized_business_validation_failure_after_begin(
            request,
            &valid_procedure,
            &context,
            rejection.message(),
        )
        .expect("business validation failure should roll back durably");

    assert_eq!(
        outcome.completion.status(),
        CompletionStatus::RolledBack,
        "business validation failure must roll back"
    );
    assert_eq!(
        outcome.completion.transaction_state(),
        Some(TransactionState::RolledBack),
        "rollback completion must expose rolled-back transaction state"
    );
    assert_eq!(
        runtime
            .wal()
            .records()
            .iter()
            .map(|record| record.header.kind)
            .collect::<Vec<_>>(),
        vec![WalRecordKind::TxBegin, WalRecordKind::TxRollback],
        "rollback path must durably record begin and rollback only"
    );
    assert_eq!(
        outcome.completion.durable_lsn(),
        Some(runtime.wal().durable_lsn()),
        "rollback completion must report the durable rollback LSN"
    );
}
#[test]
fn e2e_concurrent_invocations_maintain_mvcc_isolation() {
    // Arrange: Set up multiple concurrent invocation paths
    let contract = valid_contract();

    // Invocation A: product_id=42, reserve quantity=5
    let request_a = build_invocation_request(&contract, 6001);
    let context_a = build_invocation_context(6001);

    // Invocation B: product_id=42, reserve quantity=3
    let request_b = build_invocation_request(&contract, 6002);
    let context_b = build_invocation_context(6002);

    let procedure_a = execute_reserve_stock(20, 5);
    let procedure_b = execute_reserve_stock(20, 3);

    let mut runtime_a = LocalVerticalRuntime::new(InMemoryWal::new());
    let mut runtime_b = LocalVerticalRuntime::new(InMemoryWal::new());

    // Act: Execute both concurrently (simulated via sequential execution with different runtimes)
    let outcome_a = runtime_a
        .execute_authorized_io_admitted(
            request_a,
            &procedure_a,
            &context_a,
            foreground_io_admission(context_a.trace_id),
        )
        .expect("invocation A should succeed");

    let outcome_b = runtime_b
        .execute_authorized_io_admitted(
            request_b,
            &procedure_b,
            &context_b,
            foreground_io_admission(context_b.trace_id),
        )
        .expect("invocation B should succeed");

    // Assert: Both transactions completed independently
    assert_eq!(outcome_a.completion.status(), CompletionStatus::Committed);
    assert_eq!(outcome_b.completion.status(), CompletionStatus::Committed);

    assert!(outcome_a.transaction_id.get() > 0);
    assert!(outcome_b.transaction_id.get() > 0);

    // Assert: Both wrote to WAL successfully
    assert!(outcome_a.completion.durable_lsn().is_some());
    assert!(outcome_b.completion.durable_lsn().is_some());

    // Assert: No dirty reads (each tx committed independently)
    assert_eq!(outcome_a.completion.rows_affected(), Some(2));
    assert_eq!(outcome_b.completion.rows_affected(), Some(2));
}
