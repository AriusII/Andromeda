use super::support::*;

#[test]
fn catalog_backed_runtime_resolves_visible_procedure_before_begin() {
    let store = inventory_catalog_store();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let context = inventory_context(&contract, 7100);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

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
fn catalog_backed_runtime_rejects_absent_procedure_before_begin() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let catalog = CatalogSnapshot::empty(
        DatabaseId::new(0x1000),
        NamespaceId::new(0x1001),
        CatalogVersion::new(1),
    );
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let error = runtime
        .execute_catalog_resolved(
            inventory_request(&contract),
            &procedure,
            &catalog,
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
    store
        .apply_definition_batch(&DefinitionBatch {
            batch_id: DefinitionBatchId::new(0x7102),
            database_id: DatabaseId::new(0x1000),
            namespace_id: NamespaceId::new(0x1001),
            base_version: CatalogVersion::new(1),
            operations: vec![DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                object: contract.object.clone(),
            })],
        })
        .unwrap();
    let mut request = inventory_request(&contract);
    request.catalog_version = store.snapshot().version;
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let error = runtime
        .execute_catalog_resolved(request, &procedure, store.snapshot(), TraceId::new(7102))
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
        .execute_catalog_resolved(request, &procedure, store.snapshot(), TraceId::new(7103))
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
        .execute_catalog_resolved(request, &procedure, store.snapshot(), TraceId::new(7104))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("ContractHash mismatch"));
    assert!(runtime.wal().is_empty());
}
