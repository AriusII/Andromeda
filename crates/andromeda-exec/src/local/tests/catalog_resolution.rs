use super::support::*;

#[test]
fn catalog_backed_runtime_resolves_visible_procedure_before_begin() {
    let store = inventory_catalog_store();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let context = inventory_context(&contract, 7100);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    assert!(store.snapshot().is_durably_published());
    assert_eq!(
        store.snapshot().visible_version(),
        contract.object.catalog_version
    );

    let outcome = runtime
        .execute_authorized_catalog_resolved(
            inventory_request(&contract),
            &procedure,
            store.snapshot(),
            &context,
        )
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    assert_eq!(runtime.wal().replay_durable().len(), 3);
}

#[test]
fn local_runtime_carries_resource_budget_evidence_before_dispatch() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let context = inventory_context(&contract, 7106);
    let io_admission = foreground_io_admission(context.trace_id);
    let decision = io_admission
        .as_ref()
        .expect("foreground admission should accept resource budget");
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    assert_eq!(
        decision.resource_budget,
        ResourceBudget::new(8 * 1024 * 1024, 1024 * 1024, 2)
    );
    assert_eq!(
        decision.resource_scope,
        ResourceBudgetScope::for_procedure(contract.procedure_id)
    );
    assert_eq!(decision.trace.trace_id, context.trace_id);

    let outcome = runtime
        .execute_authorized_io_admitted(
            inventory_request_with_id(&contract, 706),
            &procedure,
            &context,
            io_admission,
        )
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    assert_eq!(runtime.wal().replay_durable().len(), 3);
}

#[test]
fn catalog_backed_runtime_rejects_staged_snapshot_before_begin() {
    let store = staged_inventory_catalog_store();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let context = inventory_context(&contract, 7105);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    assert!(!store.snapshot().is_durably_published());
    assert_eq!(store.snapshot().visible_version(), CatalogVersion::new(0));
    assert_eq!(store.snapshot().version, contract.object.catalog_version);

    let error = runtime
        .execute_authorized_catalog_resolved(
            inventory_request(&contract),
            &procedure,
            store.snapshot(),
            &context,
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("durably published"));
    assert!(runtime.wal().is_empty());
}

#[test]
fn catalog_backed_runtime_rejects_local_contract_drift_before_runtime_dispatch() {
    let store = inventory_catalog_store();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut procedure = inventory_local_procedure(&contract);
    procedure.contract.contract_hash = ContractHash::test_vector(0x06);
    let context = inventory_context(&contract, 7116);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let error = runtime
        .execute_authorized_catalog_resolved(
            inventory_request(&contract),
            &procedure,
            store.snapshot(),
            &context,
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("visible catalog contract"));
    assert!(runtime.wal().is_empty());
    assert_eq!(runtime.transactions().live_count().unwrap(), 0);
}

#[test]
fn catalog_backed_runtime_rejects_local_binding_drift_before_runtime_dispatch() {
    let store = inventory_catalog_store();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut procedure = inventory_local_procedure(&contract);
    procedure.contract_binding.stats_version = StatsVersion::new(2);
    let context = inventory_context(&contract, 7117);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let error = runtime
        .execute_authorized_catalog_resolved(
            inventory_request(&contract),
            &procedure,
            store.snapshot(),
            &context,
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("visible catalog binding"));
    assert!(runtime.wal().is_empty());
    assert_eq!(runtime.transactions().live_count().unwrap(), 0);
}

#[test]
fn catalog_backed_runtime_rejects_absent_procedure_before_begin() {
    let store = inventory_catalog_store();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let mut request = inventory_request(&contract);
    request.procedure.procedure_id = ProcedureId::new(0x7101);
    request.expected_binding.as_mut().unwrap().procedure_id = request.procedure.procedure_id;
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let error = runtime
        .execute_internal_catalog_resolved(
            request,
            &procedure,
            store.snapshot(),
            TraceId::new(7101),
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("absent"));
    assert!(runtime.wal().is_empty());
}

#[test]
fn catalog_backed_runtime_rejects_inactive_procedure_before_begin() {
    let mut store = inventory_catalog_store();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    apply_definition_batch_durably_for_test(
        &mut store,
        &DefinitionBatch {
            batch_id: DefinitionBatchId::new(0x7102),
            database_id: DatabaseId::new(0x1000),
            namespace_id: NamespaceId::new(0x1001),
            base_version: CatalogVersion::new(1),
            operations: vec![DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                object: contract.object.clone(),
            })],
        },
    );
    let mut request = inventory_request(&contract);
    request.catalog_version = store.snapshot().version;
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let error = runtime
        .execute_internal_catalog_resolved(
            request,
            &procedure,
            store.snapshot(),
            TraceId::new(7102),
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("not active"));
    assert!(runtime.wal().is_empty());
}

#[test]
fn catalog_backed_runtime_rejects_stale_catalog_version_before_begin() {
    let store = inventory_catalog_store();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let mut request = inventory_request(&contract);
    request.catalog_version = CatalogVersion::new(2);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let error = runtime
        .execute_internal_catalog_resolved(
            request,
            &procedure,
            store.snapshot(),
            TraceId::new(7103),
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("CatalogVersion mismatch"));
    assert!(runtime.wal().is_empty());
}

#[test]
fn catalog_backed_runtime_rejects_contract_hash_mismatch_before_begin() {
    let store = inventory_catalog_store();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let mut request = inventory_request(&contract);
    request.expected_contract_hash = ContractHash::test_vector(0x99);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let error = runtime
        .execute_internal_catalog_resolved(
            request,
            &procedure,
            store.snapshot(),
            TraceId::new(7104),
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("ContractHash mismatch"));
    assert!(runtime.wal().is_empty());
}

#[test]
fn local_runtime_rejects_contract_before_resource_budget_admission() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let context = inventory_context(&contract, 7118);
    let mut request = inventory_request_with_id(&contract, 718);
    request.expected_contract_hash = ContractHash::test_vector(0x08);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let error = runtime
        .execute_authorized_io_admitted(
            request,
            &procedure,
            &context,
            rejected_resource_budget_io_admission(context.trace_id),
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("ContractHash"));
    assert!(!error.message().contains("execution IO admission"));
    assert!(runtime.wal().is_empty());
    assert_eq!(runtime.transactions().live_count().unwrap(), 0);
}

#[test]
fn local_runtime_rejects_authorization_before_resource_budget_admission() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let context = InvocationContext::new(TraceId::new(7119), Vec::new());
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let error = runtime
        .execute_authorized_io_admitted(
            inventory_request_with_id(&contract, 719),
            &procedure,
            &context,
            rejected_resource_budget_io_admission(context.trace_id),
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Security);
    assert!(error.message().contains("missing required permission"));
    assert!(!error.message().contains("execution IO admission"));
    assert!(runtime.wal().is_empty());
    assert_eq!(runtime.transactions().live_count().unwrap(), 0);
}

#[test]
fn local_runtime_rejects_resource_budget_before_transaction_begin() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let context = inventory_context(&contract, 7120);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let error = runtime
        .execute_authorized_io_admitted(
            inventory_request_with_id(&contract, 720),
            &procedure,
            &context,
            rejected_resource_budget_io_admission(context.trace_id),
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Resource);
    assert!(error.message().contains("execution IO admission rejected"));
    assert!(error.message().contains("memory budget must not be zero"));
    assert!(runtime.wal().is_empty());
    assert_eq!(runtime.transactions().live_count().unwrap(), 0);
}

#[test]
fn local_runtime_rejects_job_scoped_budget_before_transaction_begin() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let context = inventory_context(&contract, 7121);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let error = runtime
        .execute_authorized_io_admitted(
            inventory_request_with_id(&contract, 721),
            &procedure,
            &context,
            job_scoped_io_admission(context.trace_id),
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("ProcedureId"));
    assert!(runtime.wal().is_empty());
    assert_eq!(runtime.transactions().live_count().unwrap(), 0);
}
