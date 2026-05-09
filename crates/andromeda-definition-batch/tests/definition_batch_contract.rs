#![forbid(unsafe_code)]

use andromeda_catalog_store::{
    CatalogDefinition, CatalogObjectRef, ObjectKind, QualifiedName, StructuredObjectDefinition,
    TableDefinition,
};
use andromeda_contract::{
    AccessMode, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy, ProcedureContract,
    ProcedureContractCandidate, ProcedureErrorPolicy, ProtocolLayoutRef, ResultMetadataPolicy,
    ResultStreamCardinality, ResultStreamContract, StatsVersion, TransactionPolicy,
};
use andromeda_definition_batch::{
    CatalogDependencyKind, CatalogLifecycleAction, CatalogLifecycleTarget, DefinitionBatch,
    DefinitionBatchId, DefinitionOperation, dry_run_definition_batch,
};
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, DatabaseId, NamespaceId,
    ProcedureId, ScalarType, TypeDescriptor,
};

const DATABASE_ID: DatabaseId = DatabaseId::new(1);
const NAMESPACE_ID: NamespaceId = NamespaceId::new(2);

fn version(value: u64) -> CatalogVersion {
    CatalogVersion::new(value)
}

fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(ScalarType::I64),
        ordinal,
    }
}

fn object_ref(
    id: u64,
    name: &str,
    kind: ObjectKind,
    catalog_version: CatalogVersion,
) -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(id),
        name: QualifiedName::parse(name).unwrap(),
        kind,
        catalog_version,
    }
}

fn table(id: u64, name: &str, catalog_version: CatalogVersion) -> TableDefinition {
    TableDefinition {
        object: object_ref(id, name, ObjectKind::Table, catalog_version),
        columns: vec![column("ProductId", 0)],
    }
}

fn structured_object(
    id: u64,
    name: &str,
    catalog_version: CatalogVersion,
) -> StructuredObjectDefinition {
    StructuredObjectDefinition {
        object: object_ref(id, name, ObjectKind::StructuredObject, catalog_version),
        fields: vec![column("ProductId", 0)],
        unique_by: vec!["ProductId".to_string()],
    }
}

fn result_stream(
    columns: Vec<ColumnDescriptor>,
    cardinality: ResultStreamCardinality,
) -> ResultStreamContract {
    ResultStreamContract {
        stream_id: 1,
        name: "Rows".to_string(),
        columns,
        cardinality,
        row_count_exact_required: cardinality.legacy_row_count_exact_required(),
    }
}

fn procedure_contract(
    id: u64,
    name: &str,
    catalog_version: CatalogVersion,
    structured_inputs: Vec<QualifiedName>,
    result_streams: Vec<ResultStreamContract>,
) -> ProcedureContract {
    ProcedureContractCandidate {
        object: object_ref(id, name, ObjectKind::Procedure, catalog_version),
        procedure_id: ProcedureId::new(id),
        stats_version: StatsVersion::new(1),
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(0xA1),
            frame_envelope_hash: ContractHash::test_vector(0xA2),
        },
        inputs: vec![column("ProductId", 0)],
        structured_inputs,
        result_streams,
        required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Serializable,
            retryable: false,
        },
        compatibility_policy: CompatibilityPolicy::AdditiveOnly,
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: vec!["InsufficientStock".to_string()],
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
    }
    .materialize()
    .unwrap()
}

fn create_table(id: u64, name: &str, catalog_version: u64) -> DefinitionOperation {
    DefinitionOperation::Create(CatalogDefinition::Table(table(
        id,
        name,
        version(catalog_version),
    )))
}

fn create_procedure(id: u64, name: &str, catalog_version: u64) -> DefinitionOperation {
    DefinitionOperation::Create(CatalogDefinition::Procedure(procedure_contract(
        id,
        name,
        version(catalog_version),
        Vec::new(),
        Vec::new(),
    )))
}

fn batch(base_version: CatalogVersion, operations: Vec<DefinitionOperation>) -> DefinitionBatch {
    DefinitionBatch {
        batch_id: DefinitionBatchId::new(base_version.get() + 100),
        database_id: DATABASE_ID,
        namespace_id: NAMESPACE_ID,
        base_version,
        operations,
    }
}

#[test]
fn source_hash_preserves_ordered_definition_batch_identity() {
    let base_version = version(10);
    let first = batch(
        base_version,
        vec![
            create_table(1, "Inventory.Product", 11),
            create_table(2, "Inventory.Stock", 11),
        ],
    );
    let reordered = batch(
        base_version,
        vec![
            create_table(2, "Inventory.Stock", 11),
            create_table(1, "Inventory.Product", 11),
        ],
    );

    assert_eq!(first.source_hash(), first.clone().source_hash());
    assert!(!first.source_hash().is_zero());
    assert_ne!(
        first.source_hash(),
        reordered.source_hash(),
        "source hash must bind the exact ordered DefinitionBatch source"
    );

    assert_eq!(
        first.dependency_graph_hash().unwrap(),
        reordered.dependency_graph_hash().unwrap(),
        "independent operations produce the same canonical dependency graph"
    );
}

#[test]
fn dependency_graph_hash_changes_with_catalog_dependency_edge() {
    let base_version = version(20);
    let catalog_version = version(21);
    let stock_request = CatalogDefinition::StructuredObject(structured_object(
        1,
        "Inventory.StockRequest",
        catalog_version,
    ));
    let audit_request = CatalogDefinition::StructuredObject(structured_object(
        2,
        "Inventory.AuditRequest",
        catalog_version,
    ));

    let stock_dependency = batch(
        base_version,
        vec![
            DefinitionOperation::Create(stock_request.clone()),
            DefinitionOperation::Create(audit_request.clone()),
            DefinitionOperation::Create(CatalogDefinition::Procedure(procedure_contract(
                3,
                "Inventory.ReserveStock",
                catalog_version,
                vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                Vec::new(),
            ))),
        ],
    );
    let audit_dependency = batch(
        base_version,
        vec![
            DefinitionOperation::Create(stock_request),
            DefinitionOperation::Create(audit_request),
            DefinitionOperation::Create(CatalogDefinition::Procedure(procedure_contract(
                3,
                "Inventory.ReserveStock",
                catalog_version,
                vec![QualifiedName::parse("Inventory.AuditRequest").unwrap()],
                Vec::new(),
            ))),
        ],
    );

    let stock_hash = stock_dependency.dependency_graph_hash().unwrap();
    let audit_hash = audit_dependency.dependency_graph_hash().unwrap();

    assert!(!stock_hash.is_zero());
    assert!(!audit_hash.is_zero());
    assert_ne!(
        stock_hash, audit_hash,
        "dependency graph hash must bind the canonical dependency edge set"
    );
}

#[test]
fn dry_run_rejects_duplicate_object_ids_and_names() {
    let duplicate_id = batch(
        version(1),
        vec![
            create_procedure(300, "test.Dup1", 2),
            create_procedure(300, "test.Dup2", 2),
        ],
    );
    let error = dry_run_definition_batch(
        duplicate_id.batch_id,
        duplicate_id.database_id,
        duplicate_id.namespace_id,
        duplicate_id.base_version,
        &duplicate_id.operations,
    )
    .unwrap_err();
    assert!(error.message().contains("same lifecycle object id twice"));

    let duplicate_name = batch(
        version(1),
        vec![
            create_procedure(300, "test.DupName", 2),
            create_procedure(301, "test.DupName", 2),
        ],
    );
    let error = dry_run_definition_batch(
        duplicate_name.batch_id,
        duplicate_name.database_id,
        duplicate_name.namespace_id,
        duplicate_name.base_version,
        &duplicate_name.operations,
    )
    .unwrap_err();
    assert!(error.message().contains("same lifecycle object name twice"));
}

#[test]
fn dry_run_rejects_empty_batch_and_zero_batch_id() {
    let empty = batch(version(1), Vec::new());
    let error = dry_run_definition_batch(
        empty.batch_id,
        empty.database_id,
        empty.namespace_id,
        empty.base_version,
        &empty.operations,
    )
    .unwrap_err();
    assert!(error.message().contains("at least one operation"));

    let zero_id = DefinitionBatch {
        batch_id: DefinitionBatchId::new(0),
        database_id: DATABASE_ID,
        namespace_id: NAMESPACE_ID,
        base_version: version(1),
        operations: vec![create_procedure(500, "test.NoBatchId", 2)],
    };
    let error = dry_run_definition_batch(
        zero_id.batch_id,
        zero_id.database_id,
        zero_id.namespace_id,
        zero_id.base_version,
        &zero_id.operations,
    )
    .unwrap_err();
    assert!(error.message().contains("batch id must not be zero"));
}

#[test]
fn deprecate_lifecycle_produces_deprecated_object_dry_run_evidence() {
    let base_version = version(4);
    let next_version = version(5);
    let target = object_ref(
        801,
        "test.DeprecatedProcedure",
        ObjectKind::Procedure,
        base_version,
    );
    let definition_batch = batch(
        base_version,
        vec![DefinitionOperation::Deprecate(CatalogLifecycleTarget {
            object: target.clone(),
        })],
    );

    let plan = dry_run_definition_batch(
        definition_batch.batch_id,
        definition_batch.database_id,
        definition_batch.namespace_id,
        definition_batch.base_version,
        &definition_batch.operations,
    )
    .unwrap();

    assert!(plan.created_objects.is_empty());
    assert_eq!(plan.deprecated_objects.len(), 1);
    let deprecated = &plan.deprecated_objects[0];
    assert_eq!(deprecated.object_id, target.object_id);
    assert_eq!(deprecated.name, target.name);
    assert_eq!(deprecated.kind, ObjectKind::Procedure);
    assert_eq!(deprecated.action, CatalogLifecycleAction::Deprecate);
    assert_eq!(deprecated.planned_version, next_version);
}

#[test]
fn dry_run_rejects_implicit_create_and_deprecate_of_same_object() {
    let base_version = version(6);
    let target = object_ref(
        802,
        "test.ReplaceWithoutExplicitAlter",
        ObjectKind::Procedure,
        base_version,
    );
    let definition_batch = batch(
        base_version,
        vec![
            create_procedure(802, "test.ReplaceWithoutExplicitAlter", 7),
            DefinitionOperation::Deprecate(CatalogLifecycleTarget { object: target }),
        ],
    );

    let error = dry_run_definition_batch(
        definition_batch.batch_id,
        definition_batch.database_id,
        definition_batch.namespace_id,
        definition_batch.base_version,
        &definition_batch.operations,
    )
    .unwrap_err();
    assert!(error.message().contains("same lifecycle object id twice"));
}

#[test]
fn source_hash_changes_when_procedure_contract_shape_changes() {
    let baseline = procedure_contract(
        1,
        "Inventory.ReserveStock",
        version(2),
        Vec::new(),
        vec![result_stream(
            vec![column("ProductId", 0)],
            ResultStreamCardinality::Many,
        )],
    );
    let changed = procedure_contract(
        1,
        "Inventory.ReserveStock",
        version(2),
        Vec::new(),
        vec![result_stream(
            vec![column("ProductId", 0), column("QuantityAvailable", 1)],
            ResultStreamCardinality::Many,
        )],
    );
    assert_ne!(baseline.contract_hash, changed.contract_hash);

    let baseline_batch = batch(
        version(1),
        vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
            baseline,
        ))],
    );
    let changed_batch = DefinitionBatch {
        operations: vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
            changed,
        ))],
        ..baseline_batch.clone()
    };

    assert_ne!(
        baseline_batch.source_hash(),
        changed_batch.source_hash(),
        "DefinitionBatch source hash is current object-version evidence for shape drift"
    );
}

#[test]
fn procedure_structured_inputs_are_definition_batch_dependencies() {
    let contract = procedure_contract(
        2,
        "Inventory.ReserveStock",
        version(11),
        vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
        Vec::new(),
    );
    let definition = CatalogDefinition::Procedure(contract);

    let dependencies = definition.dependencies();

    assert_eq!(dependencies.len(), 1);
    assert_eq!(
        dependencies[0].kind,
        CatalogDependencyKind::ProcedureStructuredInput
    );
    assert_eq!(dependencies[0].dependent_kind, ObjectKind::Procedure);
    assert_eq!(
        dependencies[0].dependency_kind,
        ObjectKind::StructuredObject
    );
    assert_eq!(
        dependencies[0].dependency_name,
        QualifiedName::parse("Inventory.StockRequest").unwrap()
    );
}
