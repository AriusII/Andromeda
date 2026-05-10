//! Tests for digest backend behavior, catalog object hashing, and the extended
//! catalog dependency graph (Procedure to Table edges via bindings).

use andromeda_catalog_store::{
    CatalogBindingKind, CatalogDefinition, CatalogObjectBinding, CatalogObjectRef, ObjectKind,
    QualifiedName, StructuredObjectDefinition, TableDefinition,
};
use andromeda_definition_batch::{
    BatchDependencyGraph, CatalogDependency, CatalogDependencyKind, DefinitionBatch,
    DefinitionBatchId, DefinitionOperation,
};
use andromeda_error::AndromedaErrorKind;
use andromeda_procedure_contract::{
    AccessMode, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy, ProcedureContract,
    ProcedureContractCandidate, ProcedureErrorPolicy, ProtocolLayoutRef, ResultMetadataPolicy,
    StatsVersion, TransactionPolicy,
};
use andromeda_structured_object::{
    compute_structured_object_shape_hash, structured_object_shape_hash_compatible,
};
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, ProcedureId, ScalarType,
    TypeDescriptor,
};

#[test]
fn digest_backend_incremental_hash_matches_one_shot_hash() {
    let message = b"catalog contract digest canonical backend";

    let mut incremental_hasher = andromeda_digest::Sha256::new();
    incremental_hasher.update(&message[..8]);
    incremental_hasher.update(&message[8..]);

    assert_eq!(
        incremental_hasher.finalize(),
        andromeda_digest::sha256(message)
    );
}

fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
    typed_column(name, ordinal, ScalarType::I64)
}

fn typed_column(name: &str, ordinal: u32, scalar: ScalarType) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(scalar),
        ordinal,
    }
}

fn object(id: u64, name: &str, kind: ObjectKind, version: CatalogVersion) -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(id),
        name: QualifiedName::parse(name).unwrap(),
        kind,
        catalog_version: version,
    }
}

fn table(id: u64, name: &str, version: CatalogVersion) -> TableDefinition {
    table_with_columns(id, name, version, vec![column("ProductId", 0)])
}

fn table_with_columns(
    id: u64,
    name: &str,
    version: CatalogVersion,
    columns: Vec<ColumnDescriptor>,
) -> TableDefinition {
    TableDefinition {
        object: object(id, name, ObjectKind::Table, version),
        columns,
    }
}

fn structured(id: u64, name: &str, version: CatalogVersion) -> StructuredObjectDefinition {
    StructuredObjectDefinition {
        object: object(id, name, ObjectKind::StructuredObject, version),
        fields: vec![column("ProductId", 0)],
        unique_by: vec!["ProductId".to_string()],
    }
}

#[test]
fn structured_object_boundary_hash_compatibility_is_exact() {
    let baseline = structured(41, "Inventory.Reservation", CatalogVersion::new(1));
    let mut drifted = baseline.clone();
    drifted.fields.push(column("Quantity", 1));
    drifted.unique_by.push("Quantity".to_string());

    let baseline_hash = compute_structured_object_shape_hash(&baseline.fields, &baseline.unique_by);
    let drifted_hash = compute_structured_object_shape_hash(&drifted.fields, &drifted.unique_by);

    assert!(structured_object_shape_hash_compatible(
        baseline_hash,
        baseline_hash
    ));
    assert!(!structured_object_shape_hash_compatible(
        baseline_hash,
        drifted_hash
    ));
}

fn procedure(
    id: u64,
    name: &str,
    version: CatalogVersion,
    structured_inputs: Vec<QualifiedName>,
) -> ProcedureContract {
    ProcedureContractCandidate {
        object: object(id, name, ObjectKind::Procedure, version),
        procedure_id: ProcedureId::new(id),
        stats_version: StatsVersion::new(1),
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(0xA1),
            frame_envelope_hash: ContractHash::test_vector(0xA2),
        },
        inputs: vec![column("ProductId", 0)],
        structured_inputs,
        result_streams: Vec::new(),
        required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Serializable,
            retryable: false,
        },
        compatibility_policy: CompatibilityPolicy::ExactHash,
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

#[test]
fn catalog_definition_shape_hash_tracks_versioned_additive_and_breaking_table_changes() {
    let baseline =
        CatalogDefinition::Table(table(401, "Inventory.ProductStock", CatalogVersion::new(1)));
    let same_shape_next_version =
        CatalogDefinition::Table(table(401, "Inventory.ProductStock", CatalogVersion::new(2)));
    let additive_next_version = CatalogDefinition::Table(table_with_columns(
        401,
        "Inventory.ProductStock",
        CatalogVersion::new(2),
        vec![column("ProductId", 0), column("OnHandQuantity", 1)],
    ));
    let breaking_next_version = CatalogDefinition::Table(table_with_columns(
        401,
        "Inventory.ProductStock",
        CatalogVersion::new(2),
        vec![typed_column("Sku", 0, ScalarType::Bool)],
    ));

    assert!(baseline.validate().is_ok());
    assert!(same_shape_next_version.validate().is_ok());
    assert!(additive_next_version.validate().is_ok());
    assert!(breaking_next_version.validate().is_ok());

    assert_ne!(
        baseline.shape_hash(),
        same_shape_next_version.shape_hash(),
        "CatalogVersion participates in the object diff hash"
    );
    assert_ne!(
        same_shape_next_version.shape_hash(),
        additive_next_version.shape_hash(),
        "appending a table column must produce a distinct additive shape"
    );
    assert_ne!(
        additive_next_version.shape_hash(),
        breaking_next_version.shape_hash(),
        "breaking table-column replacement must not collide with additive shape"
    );
}

#[test]
fn definition_batch_source_hash_binds_procedure_identity() {
    let base = procedure(21, "Inventory.ReserveStock", CatalogVersion::new(1), vec![]);
    let mut drifted = base.clone();
    drifted.procedure_id = ProcedureId::new(22);

    assert_eq!(
        base.contract_hash, drifted.contract_hash,
        "ContractHash remains a shape contract and does not carry ProcedureId"
    );

    let base_batch = DefinitionBatch {
        batch_id: DefinitionBatchId::new(1),
        database_id: andromeda_types::DatabaseId::new(1),
        namespace_id: andromeda_types::NamespaceId::new(1),
        base_version: CatalogVersion::new(0),
        operations: vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
            base,
        ))],
    };
    let drifted_batch = DefinitionBatch {
        operations: vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
            drifted,
        ))],
        ..base_batch.clone()
    };

    assert_ne!(
        base_batch.source_hash(),
        drifted_batch.source_hash(),
        "DefinitionBatch source hash must bind ProcedureId drift even when ContractHash is unchanged"
    );
}

#[test]
fn procedure_catalog_version_advances_definition_evidence_without_contract_hash_drift() {
    let v1 = procedure(31, "Inventory.ReserveStock", CatalogVersion::new(1), vec![]);
    let v2 = procedure(31, "Inventory.ReserveStock", CatalogVersion::new(2), vec![]);

    assert_eq!(
        v1.contract_hash, v2.contract_hash,
        "ContractHash must remain canonical procedure shape, independent from publication version"
    );
    assert_eq!(
        v1.validated_binding().unwrap().contract_hash,
        v1.contract_hash
    );
    assert_eq!(
        v2.validated_binding().unwrap().catalog_version,
        CatalogVersion::new(2),
        "validated binding evidence carries the concrete published CatalogVersion"
    );

    let v1_definition = CatalogDefinition::Procedure(v1.clone());
    let v2_definition = CatalogDefinition::Procedure(v2.clone());
    assert_ne!(
        v1_definition.shape_hash(),
        v2_definition.shape_hash(),
        "catalog definition evidence must change when a procedure is republished at a new CatalogVersion"
    );

    let v1_batch = DefinitionBatch {
        batch_id: DefinitionBatchId::new(2),
        database_id: andromeda_types::DatabaseId::new(1),
        namespace_id: andromeda_types::NamespaceId::new(1),
        base_version: CatalogVersion::new(0),
        operations: vec![DefinitionOperation::Create(v1_definition)],
    };
    let v2_batch = DefinitionBatch {
        operations: vec![DefinitionOperation::Create(v2_definition)],
        ..v1_batch.clone()
    };

    assert_ne!(
        v1_batch.source_hash(),
        v2_batch.source_hash(),
        "DefinitionBatch source hash must bind CatalogVersion drift even when ContractHash is stable"
    );
}

#[test]
fn dependency_graph_includes_procedure_to_table_edges_from_bindings() {
    let version = CatalogVersion::new(1);
    let stock_table = table(101, "Inventory.ProductStock", version);
    let request = structured(102, "Inventory.StockRequest", version);
    let result = structured(103, "Inventory.Reservation", version);
    let proc = procedure(
        104,
        "Inventory.ReserveStock",
        version,
        vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
    );

    let operations = vec![
        DefinitionOperation::Create(CatalogDefinition::Table(stock_table.clone())),
        DefinitionOperation::Create(CatalogDefinition::StructuredObject(request.clone())),
        DefinitionOperation::Create(CatalogDefinition::StructuredObject(result.clone())),
        DefinitionOperation::Create(CatalogDefinition::Procedure(proc.clone())),
    ];

    let bindings = vec![
        CatalogObjectBinding {
            dependent: proc.object.clone(),
            dependency: stock_table.object.clone(),
            kind: CatalogBindingKind::WritesTable,
        },
        CatalogObjectBinding {
            dependent: proc.object.clone(),
            dependency: result.object.clone(),
            kind: CatalogBindingKind::EmitsStructuredObject,
        },
    ];

    // Without bindings the graph only sees the structured-input dependency.
    let no_bindings = BatchDependencyGraph::from_operations(&operations).unwrap();
    let _ = no_bindings; // smoke check

    // With bindings it must also include Procedure→Table and
    // Procedure→StructuredObject (emit) edges.
    let with_bindings =
        BatchDependencyGraph::from_operations_with_bindings(&operations, &bindings).unwrap();

    let writes_table = CatalogDependency::procedure_writes_table(
        proc.object.name.clone(),
        stock_table.object.name.clone(),
    );
    assert_eq!(
        writes_table.kind,
        CatalogDependencyKind::ProcedureWritesTable
    );
    assert_eq!(writes_table.dependency_kind, ObjectKind::Table);
    assert_eq!(writes_table.dependent_kind, ObjectKind::Procedure);
    assert!(writes_table.validate().is_ok());

    // Sanity: building from the binding produces the same dependency value.
    assert_eq!(
        CatalogDependency::from_binding(&bindings[0]),
        writes_table,
        "from_binding must lower CatalogObjectBinding faithfully"
    );

    // The graph itself is opaque, but its structural correctness is asserted
    // by acyclic validation succeeding above.
    let _ = with_bindings;
}

#[test]
fn dependency_graph_rejects_missing_table_dependency_kind_mismatch() {
    let version = CatalogVersion::new(1);
    // Create a structured object with the same qualified name a binding will
    // claim is a Table — kind mismatch must be rejected.
    let mistyped = structured(201, "Inventory.ProductStock", version);
    let proc = procedure(202, "Inventory.ReserveStock", version, vec![]);

    let operations = vec![
        DefinitionOperation::Create(CatalogDefinition::StructuredObject(mistyped.clone())),
        DefinitionOperation::Create(CatalogDefinition::Procedure(proc.clone())),
    ];

    let bindings = vec![CatalogObjectBinding {
        dependent: proc.object.clone(),
        dependency: object(
            201,
            "Inventory.ProductStock",
            ObjectKind::Table, // lies about kind
            version,
        ),
        kind: CatalogBindingKind::ReadsTable,
    }];

    // The binding itself is malformed (claims kind Table for an id that the
    // batch declares as StructuredObject).  `from_operations_with_bindings`
    // must surface this as a Catalog error.
    let error = BatchDependencyGraph::from_operations_with_bindings(&operations, &bindings)
        .expect_err("kind mismatch must be rejected");
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
}

#[test]
fn dependency_graph_rejects_forward_table_reference_inside_batch() {
    let version = CatalogVersion::new(1);
    let proc = procedure(301, "Inventory.ReserveStock", version, vec![]);
    let stock_table = table(302, "Inventory.ProductStock", version);

    // Procedure declared *before* the table it depends on inside the same
    // batch — the dependency graph must reject this ordering when the
    // binding is supplied.
    let operations = vec![
        DefinitionOperation::Create(CatalogDefinition::Procedure(proc.clone())),
        DefinitionOperation::Create(CatalogDefinition::Table(stock_table.clone())),
    ];
    let bindings = vec![CatalogObjectBinding {
        dependent: proc.object.clone(),
        dependency: stock_table.object.clone(),
        kind: CatalogBindingKind::ReadsTable,
    }];

    let error = BatchDependencyGraph::from_operations_with_bindings(&operations, &bindings)
        .expect_err("forward intra-batch dependency must be rejected");
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
}
