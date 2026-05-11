#![forbid(unsafe_code)]

use andromeda_catalog_store::{
    CatalogBindingKind, CatalogDefinition, CatalogObjectBinding, CatalogObjectRef, ObjectKind,
    QualifiedName, StructuredObjectDefinition, TableDefinition,
};
use andromeda_definition_batch::{
    BatchDependencyGraph, CatalogDependency, CatalogDependencyKind, CatalogLifecycleAction,
    CatalogLifecycleTarget, DefinitionBatch, DefinitionBatchDependencyGraphHash, DefinitionBatchId,
    DefinitionBatchSourceHash, DefinitionOperation, dry_run_definition_batch,
};
use andromeda_error::AndromedaErrorKind;
use andromeda_procedure_contract::{
    AccessMode, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy, ProcedureContract,
    ProcedureContractCandidate, ProcedureErrorPolicy, ProtocolLayoutRef, ResultMetadataPolicy,
    ResultStreamCardinality, ResultStreamContract, StatsVersion, TransactionPolicy,
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
fn source_hash_binds_procedure_identity_even_when_contract_hash_matches() {
    let base = procedure_contract(
        21,
        "Inventory.ReserveStock",
        version(2),
        Vec::new(),
        Vec::new(),
    );
    let mut drifted = base.clone();
    drifted.procedure_id = ProcedureId::new(22);

    assert_eq!(
        base.contract_hash, drifted.contract_hash,
        "ContractHash remains a shape contract and does not carry ProcedureId"
    );

    let base_batch = batch(
        version(1),
        vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
            base,
        ))],
    );
    let drifted_batch = DefinitionBatch {
        operations: vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
            drifted,
        ))],
        ..base_batch.clone()
    };

    assert_ne!(
        base_batch.source_hash(),
        drifted_batch.source_hash(),
        "DefinitionBatch source hash must bind ProcedureId drift"
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

#[test]
fn dependency_graph_includes_procedure_to_table_edges_from_bindings() {
    let catalog_version = version(31);
    let stock_table = table(101, "Inventory.ProductStock", catalog_version);
    let request = structured_object(102, "Inventory.StockRequest", catalog_version);
    let result = structured_object(103, "Inventory.Reservation", catalog_version);
    let procedure = procedure_contract(
        104,
        "Inventory.ReserveStock",
        catalog_version,
        vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
        Vec::new(),
    );

    let operations = vec![
        DefinitionOperation::Create(CatalogDefinition::Table(stock_table.clone())),
        DefinitionOperation::Create(CatalogDefinition::StructuredObject(request)),
        DefinitionOperation::Create(CatalogDefinition::StructuredObject(result.clone())),
        DefinitionOperation::Create(CatalogDefinition::Procedure(procedure.clone())),
    ];
    let bindings = vec![
        CatalogObjectBinding {
            dependent: procedure.object.clone(),
            dependency: stock_table.object.clone(),
            kind: CatalogBindingKind::WritesTable,
        },
        CatalogObjectBinding {
            dependent: procedure.object.clone(),
            dependency: result.object.clone(),
            kind: CatalogBindingKind::EmitsStructuredObject,
        },
    ];

    let without_bindings = BatchDependencyGraph::from_operations(&operations).unwrap();
    let with_bindings =
        BatchDependencyGraph::from_operations_with_bindings(&operations, &bindings).unwrap();
    assert!(with_bindings.dependency_count() > without_bindings.dependency_count());

    let writes_table = CatalogDependency::procedure_writes_table(
        procedure.object.name.clone(),
        stock_table.object.name.clone(),
    );
    assert_eq!(CatalogDependency::from_binding(&bindings[0]), writes_table);
    assert_eq!(
        writes_table.kind,
        CatalogDependencyKind::ProcedureWritesTable
    );
    assert_eq!(writes_table.dependent_kind, ObjectKind::Procedure);
    assert_eq!(writes_table.dependency_kind, ObjectKind::Table);
}

#[test]
fn dependency_graph_rejects_kind_mismatch_and_forward_binding_references() {
    let catalog_version = version(41);
    let mistyped = structured_object(201, "Inventory.ProductStock", catalog_version);
    let procedure = procedure_contract(
        202,
        "Inventory.ReserveStock",
        catalog_version,
        Vec::new(),
        Vec::new(),
    );
    let operations = vec![
        DefinitionOperation::Create(CatalogDefinition::StructuredObject(mistyped)),
        DefinitionOperation::Create(CatalogDefinition::Procedure(procedure.clone())),
    ];
    let bindings = vec![CatalogObjectBinding {
        dependent: procedure.object.clone(),
        dependency: object_ref(
            201,
            "Inventory.ProductStock",
            ObjectKind::Table,
            catalog_version,
        ),
        kind: CatalogBindingKind::ReadsTable,
    }];

    let error = BatchDependencyGraph::from_operations_with_bindings(&operations, &bindings)
        .expect_err("kind mismatch must be rejected");
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);

    let stock_table = table(302, "Inventory.ProductStock", catalog_version);
    let procedure = procedure_contract(
        301,
        "Inventory.ForwardRead",
        catalog_version,
        Vec::new(),
        Vec::new(),
    );
    let operations = vec![
        DefinitionOperation::Create(CatalogDefinition::Procedure(procedure.clone())),
        DefinitionOperation::Create(CatalogDefinition::Table(stock_table.clone())),
    ];
    let bindings = vec![CatalogObjectBinding {
        dependent: procedure.object.clone(),
        dependency: stock_table.object.clone(),
        kind: CatalogBindingKind::ReadsTable,
    }];

    let error = BatchDependencyGraph::from_operations_with_bindings(&operations, &bindings)
        .expect_err("forward intra-batch dependency must be rejected");
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
}

#[test]
fn validate_integrity_hashes_rejects_stale_source_or_dependency_graph_hash() {
    let definition_batch = batch(
        version(50),
        vec![
            create_table(1, "Inventory.Product", 51),
            create_table(2, "Inventory.Stock", 51),
        ],
    );

    definition_batch
        .validate_integrity_hashes(
            definition_batch.source_hash(),
            definition_batch.dependency_graph_hash().unwrap(),
        )
        .unwrap();

    let source_error = definition_batch
        .validate_integrity_hashes(
            DefinitionBatchSourceHash::new([0xAB; DefinitionBatchSourceHash::LEN]),
            definition_batch.dependency_graph_hash().unwrap(),
        )
        .expect_err("stale source hash must be rejected");
    assert_eq!(source_error.kind(), AndromedaErrorKind::Catalog);
    assert!(source_error.message().contains("source hash"));

    let dependency_error = definition_batch
        .validate_integrity_hashes(
            definition_batch.source_hash(),
            DefinitionBatchDependencyGraphHash::new(
                [0xCD; DefinitionBatchDependencyGraphHash::LEN],
            ),
        )
        .expect_err("stale dependency graph hash must be rejected");
    assert_eq!(dependency_error.kind(), AndromedaErrorKind::Catalog);
    assert!(dependency_error.message().contains("dependency graph hash"));
}
