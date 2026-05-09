use std::sync::Arc;

use andromeda_catalog::{
    CatalogDurabilityMarker, CatalogManifestResolutionRequest, CatalogManifestResolutionStatus,
    CatalogManifestStore, CatalogMutationCommitEvidence, CatalogMutationDurability,
    CatalogRuntimeReopenEvidence, CatalogServerRuntime, CatalogSnapshotManifestStore,
    CatalogSystemStore, INVENTORY_DATABASE_ID, INVENTORY_NAMESPACE_ID,
    INVENTORY_RESERVE_STOCK_PROCEDURE_ID, ProcedureContract, QualifiedName,
    inventory_domain_definition_batch, inventory_reserve_stock_contract_candidate,
};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

fn catalog_server_runtime_fixture() -> (
    CatalogServerRuntime,
    Arc<CatalogSnapshotManifestStore>,
    ProcedureContract,
) {
    let mut system_store = CatalogSystemStore::empty(
        INVENTORY_DATABASE_ID,
        INVENTORY_NAMESPACE_ID,
        CatalogVersion::new(0),
    );
    let batch = inventory_domain_definition_batch().unwrap();
    let plan = system_store.plan_definition_batch(&batch).unwrap();
    let records = plan.mutation_plan.records();
    let commit_record = records.last().unwrap();
    let commit_evidence = CatalogMutationCommitEvidence::from_durable_commit_record(
        commit_record,
        plan.mutation_plan.record_count(),
        CatalogMutationDurability::ExternalMarker(CatalogDurabilityMarker::new(99)),
    )
    .unwrap();

    system_store
        .publish_durable_mutation_plan(&plan.mutation_plan, commit_evidence)
        .unwrap();

    let manifest_store = Arc::new(CatalogSnapshotManifestStore::with_reopen_evidence(
        system_store,
        CatalogRuntimeReopenEvidence::new(99, "catalog-server-runtime-contract"),
    ));
    let runtime_store: Arc<dyn CatalogManifestStore> = manifest_store.clone();
    let runtime = CatalogServerRuntime::new(runtime_store);
    let contract = inventory_reserve_stock_contract_candidate(CatalogVersion::new(1))
        .materialize()
        .unwrap();

    (runtime, manifest_store, contract)
}

#[test]
fn catalog_server_resolves_manifest_by_id() {
    let (runtime, _store, contract) = catalog_server_runtime_fixture();

    let by_id = runtime.resolve_manifest(
        CatalogManifestResolutionRequest::by_id(INVENTORY_RESERVE_STOCK_PROCEDURE_ID)
            .with_expected_contract_hash(contract.contract_hash)
            .with_expected_catalog_version(CatalogVersion::new(1)),
    );

    assert_eq!(by_id.status, CatalogManifestResolutionStatus::Resolved);
    let manifest = by_id.manifest.unwrap();
    assert_eq!(manifest.procedure_id, INVENTORY_RESERVE_STOCK_PROCEDURE_ID);
    assert_eq!(manifest.qualified_name, "Inventory.ReserveStock");
    assert_eq!(manifest.catalog_version, CatalogVersion::new(1));
    assert_eq!(manifest.contract_hash, contract.contract_hash.as_bytes());
    assert_eq!(manifest.input_schema.len(), 2);
    assert_eq!(manifest.output_schema.len(), 1);

    let by_name = runtime.resolve_manifest_by_name("Inventory.ReserveStock");
    assert_eq!(by_name.status, CatalogManifestResolutionStatus::Resolved);

    let by_boundary_normalized_name =
        runtime.resolve_manifest_by_name(" Inventory . ReserveStock ");
    assert_eq!(
        by_boundary_normalized_name.status,
        CatalogManifestResolutionStatus::Resolved
    );

    let by_qualified_name = runtime.resolve_manifest_by_qualified_name(
        QualifiedName::parse("Inventory.ReserveStock").unwrap(),
    );
    assert_eq!(
        by_qualified_name.status,
        CatalogManifestResolutionStatus::Resolved
    );
}

#[test]
fn catalog_server_rejects_hash_mismatch_before_runtime_readiness_gate() {
    let (runtime, store, _contract) = catalog_server_runtime_fixture();
    store
        .set_source_generator_ready(INVENTORY_RESERVE_STOCK_PROCEDURE_ID, false)
        .unwrap();

    let resolution = runtime.resolve_manifest(
        CatalogManifestResolutionRequest::by_id(INVENTORY_RESERVE_STOCK_PROCEDURE_ID)
            .with_expected_contract_hash(ContractHash::test_vector(0xCC))
            .requiring_source_generator_ready(),
    );

    assert_eq!(
        resolution.status,
        CatalogManifestResolutionStatus::ContractHashMismatch
    );
    assert!(resolution.manifest.is_none());
    assert_eq!(
        resolution.diagnostic_code,
        Some("STATUS_CONTRACT_HASH_MISMATCH")
    );
}

#[test]
fn catalog_server_rejects_catalog_version_mismatch_before_runtime_readiness_gate() {
    let (runtime, store, _contract) = catalog_server_runtime_fixture();
    store
        .set_source_generator_ready(INVENTORY_RESERVE_STOCK_PROCEDURE_ID, false)
        .unwrap();

    let resolution = runtime.resolve_manifest(
        CatalogManifestResolutionRequest::by_id(INVENTORY_RESERVE_STOCK_PROCEDURE_ID)
            .with_expected_catalog_version(CatalogVersion::new(2))
            .requiring_source_generator_ready(),
    );

    assert_eq!(
        resolution.status,
        CatalogManifestResolutionStatus::CatalogVersionMismatch
    );
    assert!(resolution.manifest.is_none());
    assert_eq!(
        resolution.diagnostic_code,
        Some("STATUS_CATALOG_VERSION_MISMATCH")
    );
}

#[test]
fn catalog_server_maps_not_found_to_runtime_status() {
    let (runtime, _store, _contract) = catalog_server_runtime_fixture();

    let resolution = runtime.resolve_manifest_by_id(ProcedureId::new(999_999));

    assert_eq!(resolution.status, CatalogManifestResolutionStatus::NotFound);
    assert!(resolution.manifest.is_none());
    assert_eq!(resolution.diagnostic_code, Some("STATUS_NOT_FOUND"));
    assert_eq!(resolution.current_catalog_version, CatalogVersion::new(1));
    assert_eq!(resolution.status.diagnostic_code(), "STATUS_NOT_FOUND");
}

#[test]
fn catalog_server_rejects_not_ready_manifest() {
    let (runtime, store, _contract) = catalog_server_runtime_fixture();
    store
        .set_source_generator_ready(INVENTORY_RESERVE_STOCK_PROCEDURE_ID, false)
        .unwrap();

    let resolution = runtime.resolve_manifest(
        CatalogManifestResolutionRequest::by_id(INVENTORY_RESERVE_STOCK_PROCEDURE_ID)
            .requiring_source_generator_ready(),
    );

    assert_eq!(
        resolution.status,
        CatalogManifestResolutionStatus::NotSourceGeneratorReady
    );
    assert!(resolution.manifest.is_none());
    assert_eq!(
        resolution.diagnostic_code,
        Some("STATUS_NOT_SOURCE_GENERATOR_READY")
    );
    assert_eq!(
        resolution.status.diagnostic_code(),
        "STATUS_NOT_SOURCE_GENERATOR_READY"
    );

    let resolution_without_gate =
        runtime.resolve_manifest_by_id(INVENTORY_RESERVE_STOCK_PROCEDURE_ID);
    assert_eq!(
        resolution_without_gate.status,
        CatalogManifestResolutionStatus::Resolved
    );
}
