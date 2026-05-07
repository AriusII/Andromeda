use super::support::*;

#[test]
fn local_vertical_runtime_commits_only_after_durable_wal() {
    let mut runtime = LocalVerticalRuntime::new(TestWal::default());
    let procedure = simple_local_procedure();

    let outcome = runtime
        .execute(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(99),
        )
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    assert_eq!(
        outcome.completion.transaction_state,
        Some(TransactionState::Committed)
    );
    assert_eq!(outcome.completion.durable_lsn, Some(Lsn::new(3)));
    assert_eq!(runtime.wal().records.len(), 3);
    assert!(outcome.contract_trace.has_explanation());
}

#[test]
fn local_vertical_runtime_uses_inventory_contract_and_in_memory_wal() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let outcome = runtime
        .execute_authorized(
            inventory_request(&contract),
            &procedure,
            &inventory_context(&contract, 7000),
        )
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    assert!(outcome.authorization_trace.is_some());
    assert_eq!(outcome.completion.durable_lsn, Some(Lsn::new(3)));
    assert_eq!(runtime.wal().durable_lsn(), Lsn::new(3));
    assert_eq!(runtime.wal().replay_durable().len(), 3);

    let runtime_record = outcome.procedure_runtime_record();
    assert_eq!(runtime_record.invocation_id, InvocationId::new(700));
    assert_eq!(runtime_record.binding, contract.binding());
    assert_eq!(runtime_record.status, ProcedureRuntimeStatus::Committed);
    assert_eq!(runtime_record.error_kind, None);
    assert!(runtime_record.completed_at >= runtime_record.started_at);
    assert_eq!(
        runtime_record.duration_millis,
        runtime_record.completed_at.as_unix_millis() - runtime_record.started_at.as_unix_millis()
    );
    assert_eq!(runtime_record.counters.rows_read, 1);
    assert_eq!(runtime_record.counters.rows_written, 1);
    assert!(runtime_record.counters.wal_bytes > 0);
    assert_eq!(runtime_record.counters.temp_bytes, 0);
    let plan_key = runtime_record
        .plan_key
        .expect("local runtime emits a singleton plan key");
    assert_eq!(plan_key.catalog_version, contract.object.catalog_version);
    assert_eq!(plan_key.contract_hash, contract.contract_hash);
    assert_eq!(
        runtime_record.plan_id,
        Some(ProcedureRuntimePlanId::from_plan_cache_key(&plan_key))
    );

    let mut procedure_store = ProcedureStore::new();
    procedure_store
        .register(ProcedureStoreEntry::from_contract(&contract).unwrap())
        .unwrap();
    assert_eq!(
        outcome
            .attach_runtime_to_procedure_store(&mut procedure_store)
            .unwrap(),
        InvocationRuntimeRecordOutcome::Stored
    );
    assert_eq!(
        outcome
            .attach_runtime_to_procedure_store(&mut procedure_store)
            .unwrap(),
        InvocationRuntimeRecordOutcome::Duplicate
    );
    assert_eq!(procedure_store.total_recorded_runtime_invocations(), 1);
}
