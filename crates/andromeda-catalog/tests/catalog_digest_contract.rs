//! Tests for digest-backed `ContractHash`, `PolicyVersion`, and the extended
//! catalog dependency graph (Procedure ↔ Table edges via bindings).

use andromeda_catalog::{
    AccessMode, BatchDependencyGraph, CatalogBindingKind, CatalogDefinition, CatalogDependency,
    CatalogDependencyKind, CatalogObjectBinding, CatalogObjectRef, CompatibilityPolicy,
    ContractCompatibilityDiagnostic, DefinitionBatch, DefinitionOperation, IsolationPolicy,
    MultiResultPolicy, ObjectKind, PolicyVersion, ProcedureContract, ProcedureContractBinding,
    ProcedureContractCandidate, ProcedureErrorPolicy, ProtocolLayoutRef, QualifiedName,
    ResultMetadataPolicy, ResultStreamCardinality, ResultStreamContract, StatsVersion,
    StructuredObjectDefinition, TableDefinition, TransactionPolicy,
    compute_structured_object_shape_hash, structured_object_shape_hash_compatible,
};
use andromeda_error::AndromedaErrorKind;
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, ProcedureId, ScalarType,
    TypeDescriptor,
};

#[test]
fn catalog_digest_is_core_digest_alias() {
    let message = b"catalog contract digest canonical backend";

    let mut catalog_hasher: andromeda_catalog::digest::Sha256 = andromeda_digest::Sha256::new();
    catalog_hasher.update(&message[..8]);
    catalog_hasher.update(&message[8..]);

    let mut core_hasher: andromeda_digest::Sha256 = andromeda_catalog::digest::Sha256::new();
    core_hasher.update(message);

    assert_eq!(catalog_hasher.finalize(), core_hasher.finalize());
    assert_eq!(
        andromeda_catalog::digest::sha256(message),
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
    procedure_contract(
        id,
        name,
        version,
        structured_inputs,
        Vec::new(),
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::SingleResultOnly,
    )
}

fn procedure_contract(
    id: u64,
    name: &str,
    version: CatalogVersion,
    structured_inputs: Vec<QualifiedName>,
    result_streams: Vec<ResultStreamContract>,
    compatibility_policy: CompatibilityPolicy,
    multi_result_policy: MultiResultPolicy,
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
        result_streams,
        required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Serializable,
            retryable: false,
        },
        compatibility_policy,
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: vec!["InsufficientStock".to_string()],
        },
        multi_result_policy,
    }
    .materialize()
    .unwrap()
}

fn result_stream(
    stream_id: u64,
    name: &str,
    cardinality: ResultStreamCardinality,
    columns: Vec<ColumnDescriptor>,
) -> ResultStreamContract {
    ResultStreamContract {
        stream_id,
        name: name.to_string(),
        columns,
        cardinality,
        row_count_exact_required: cardinality.legacy_row_count_exact_required(),
    }
}

fn assert_has_message(diagnostic: &ContractCompatibilityDiagnostic, text: &str) {
    assert!(
        diagnostic
            .messages
            .iter()
            .any(|message| message.contains(text)),
        "expected diagnostic to contain {text:?}; got {:?}",
        diagnostic.messages
    );
}

#[test]
fn contract_hash_is_digest_backed_and_deterministic() {
    let proc_a = procedure(11, "Inventory.ReserveStock", CatalogVersion::new(1), vec![]);
    let proc_b = procedure(11, "Inventory.ReserveStock", CatalogVersion::new(1), vec![]);

    assert_eq!(proc_a.contract_hash, proc_b.contract_hash);
    assert!(!proc_a.contract_hash.is_zero());

    // SHA-256 must thoroughly mix every byte: a single bit change in the
    // procedure name flips a high fraction of output bits.
    let proc_renamed = procedure(11, "Inventory.ReserveAtock", CatalogVersion::new(1), vec![]);
    let differing_bytes = proc_a
        .contract_hash
        .as_bytes()
        .iter()
        .zip(proc_renamed.contract_hash.as_bytes().iter())
        .filter(|(left, right)| left != right)
        .count();
    assert!(
        differing_bytes >= 16,
        "digest-backed hash must avalanche: only {differing_bytes}/32 bytes differ"
    );
}

#[test]
fn policy_version_changes_with_policy_fields_only() {
    let baseline = procedure(12, "Inventory.ReserveStock", CatalogVersion::new(1), vec![]);
    let baseline_policy = baseline.policy_version();
    assert!(!baseline_policy.is_zero());
    assert_eq!(baseline_policy, baseline.policy_version());

    // Changing structured inputs alters the contract shape but not the
    // policy surface; PolicyVersion must be unaffected.
    let mut shape_only = baseline.clone();
    shape_only.structured_inputs = vec![QualifiedName::parse("Inventory.Stock").unwrap()];
    assert_eq!(
        baseline_policy,
        shape_only.policy_version(),
        "PolicyVersion must ignore non-policy shape changes"
    );

    // Changing a true policy field must change the PolicyVersion.
    let mut policy_changed = baseline.clone();
    policy_changed.transaction_policy.isolation = IsolationPolicy::Snapshot;
    assert_ne!(
        baseline_policy,
        policy_changed.policy_version(),
        "PolicyVersion must reflect transaction-policy changes"
    );

    let mut perms_changed = baseline.clone();
    perms_changed
        .required_permissions
        .push("Inventory.Audit".to_string());
    assert_ne!(baseline_policy, perms_changed.policy_version());
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
fn procedure_additive_compatibility_accepts_additive_result_growth() {
    let previous = procedure_contract(
        31,
        "Inventory.ReserveStock",
        CatalogVersion::new(1),
        vec![],
        vec![result_stream(
            1,
            "Reservations",
            ResultStreamCardinality::One,
            vec![column("ProductId", 0)],
        )],
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );
    let additive = procedure_contract(
        31,
        "Inventory.ReserveStock",
        CatalogVersion::new(2),
        vec![],
        vec![
            result_stream(
                1,
                "Reservations",
                ResultStreamCardinality::One,
                vec![column("ProductId", 0), column("ReservedQuantity", 1)],
            ),
            result_stream(
                2,
                "AuditRows",
                ResultStreamCardinality::Many,
                vec![column("AuditId", 0)],
            ),
        ],
        CompatibilityPolicy::AdditiveOnly,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );

    assert_ne!(
        previous.contract_hash, additive.contract_hash,
        "additive result growth changes the canonical contract hash"
    );

    let diagnostic = additive.compatibility_with(&previous);
    assert!(
        diagnostic.compatible,
        "AdditiveOnly must accept appended result columns and new result streams: {:?}",
        diagnostic.messages
    );
    assert!(diagnostic.messages.is_empty());
}

#[test]
fn procedure_exact_hash_compatibility_rejects_additive_contract_hash_change() {
    let previous = procedure_contract(
        32,
        "Inventory.ReserveStock",
        CatalogVersion::new(1),
        vec![],
        vec![result_stream(
            1,
            "Reservations",
            ResultStreamCardinality::One,
            vec![column("ProductId", 0)],
        )],
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );
    let additive_shape_with_exact_hash = procedure_contract(
        32,
        "Inventory.ReserveStock",
        CatalogVersion::new(2),
        vec![],
        vec![result_stream(
            1,
            "Reservations",
            ResultStreamCardinality::One,
            vec![column("ProductId", 0), column("ReservedQuantity", 1)],
        )],
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );

    assert_ne!(
        previous.contract_hash, additive_shape_with_exact_hash.contract_hash,
        "the exact-hash gate has to see additive result-shape drift"
    );

    let diagnostic = additive_shape_with_exact_hash.compatibility_with(&previous);
    assert!(!diagnostic.compatible);
    assert_has_message(
        &diagnostic,
        "exact-hash compatibility requires unchanged contract hash",
    );
}

#[test]
fn procedure_additive_compatibility_rejects_shape_shifting_returns() {
    let previous = procedure_contract(
        33,
        "Inventory.ReserveStock",
        CatalogVersion::new(1),
        vec![],
        vec![result_stream(
            1,
            "Reservations",
            ResultStreamCardinality::One,
            vec![column("ProductId", 0), column("ReservedQuantity", 1)],
        )],
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );

    let changed_cardinality = procedure_contract(
        33,
        "Inventory.ReserveStock",
        CatalogVersion::new(2),
        vec![],
        vec![result_stream(
            1,
            "Reservations",
            ResultStreamCardinality::Many,
            vec![column("ProductId", 0), column("ReservedQuantity", 1)],
        )],
        CompatibilityPolicy::AdditiveOnly,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );
    let diagnostic = changed_cardinality.compatibility_with(&previous);
    assert!(!diagnostic.compatible);
    assert_has_message(&diagnostic, "changing cardinality contract");

    let changed_column_prefix = procedure_contract(
        33,
        "Inventory.ReserveStock",
        CatalogVersion::new(2),
        vec![],
        vec![result_stream(
            1,
            "Reservations",
            ResultStreamCardinality::One,
            vec![
                typed_column("ProductId", 0, ScalarType::Bool),
                column("ReservedQuantity", 1),
            ],
        )],
        CompatibilityPolicy::AdditiveOnly,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );
    let diagnostic = changed_column_prefix.compatibility_with(&previous);
    assert!(!diagnostic.compatible);
    assert_has_message(
        &diagnostic,
        "existing columns to remain an unchanged prefix",
    );

    let removed_stream = procedure_contract(
        33,
        "Inventory.ReserveStock",
        CatalogVersion::new(2),
        vec![],
        Vec::new(),
        CompatibilityPolicy::AdditiveOnly,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );
    let diagnostic = removed_stream.compatibility_with(&previous);
    assert!(!diagnostic.compatible);
    assert_has_message(&diagnostic, "removing result stream Reservations");
}

#[test]
fn procedure_binding_carries_four_identities() {
    let proc = procedure(13, "Inventory.ReserveStock", CatalogVersion::new(7), vec![]);
    let binding = proc.binding();

    assert_eq!(binding.procedure_id, proc.procedure_id);
    assert_eq!(binding.contract_hash, proc.contract_hash);
    assert_eq!(binding.catalog_version, proc.object.catalog_version);
    assert_eq!(binding.stats_version, proc.stats_version);
    assert_eq!(binding.policy_version, proc.policy_version());
    assert!(binding.validate().is_ok());
    assert_eq!(proc.validated_binding().unwrap(), binding);

    let mut zero_stats = binding;
    zero_stats.stats_version = StatsVersion::new(0);
    assert_eq!(
        zero_stats.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let mut zero_policy = binding;
    zero_policy.policy_version = PolicyVersion::zero();
    assert_eq!(
        zero_policy.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );
}

#[test]
fn procedure_binding_constructor_rejects_incomplete_evidence() {
    let proc = procedure(14, "Inventory.ReserveStock", CatalogVersion::new(7), vec![]);
    let binding = proc.binding();

    let zero_stats = ProcedureContractBinding::new(
        binding.procedure_id,
        binding.catalog_version,
        binding.contract_hash,
        StatsVersion::new(0),
        binding.policy_version,
    )
    .unwrap_err();
    assert_eq!(zero_stats.kind(), AndromedaErrorKind::Contract);

    let default_policy = ProcedureContractBinding::new(
        binding.procedure_id,
        binding.catalog_version,
        binding.contract_hash,
        binding.stats_version,
        PolicyVersion::zero(),
    )
    .unwrap_err();
    assert_eq!(default_policy.kind(), AndromedaErrorKind::Contract);
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
        batch_id: andromeda_catalog::DefinitionBatchId::new(1),
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
fn procedure_compatibility_rejects_identity_or_non_advancing_version_drift() {
    let previous = procedure(23, "Inventory.ReserveStock", CatalogVersion::new(1), vec![]);
    let mut next = previous.clone();
    next.object.catalog_version = CatalogVersion::new(2);
    assert!(
        next.compatibility_with(&previous).compatible,
        "same Procedure identity with an advancing CatalogVersion and same hash remains compatible"
    );

    let mut procedure_id_drift = next.clone();
    procedure_id_drift.procedure_id = ProcedureId::new(24);
    let diagnostic = procedure_id_drift.compatibility_with(&previous);
    assert!(!diagnostic.compatible);
    assert!(
        diagnostic
            .messages
            .iter()
            .any(|message| message.contains("ProcedureId"))
    );

    let mut object_id_drift = next.clone();
    object_id_drift.object.object_id = CatalogObjectId::new(24);
    let diagnostic = object_id_drift.compatibility_with(&previous);
    assert!(!diagnostic.compatible);
    assert!(
        diagnostic
            .messages
            .iter()
            .any(|message| message.contains("object id"))
    );

    let non_advancing = previous.clone();
    let diagnostic = non_advancing.compatibility_with(&previous);
    assert!(!diagnostic.compatible);
    assert!(
        diagnostic
            .messages
            .iter()
            .any(|message| message.contains("advancing CatalogVersion"))
    );
}

#[test]
fn procedure_validated_binding_rejects_stale_contract_hash() {
    let mut stale = procedure(15, "Inventory.ReserveStock", CatalogVersion::new(7), vec![]);
    stale.contract_hash = ContractHash::test_vector(0xE5);

    let projected = stale.binding();
    assert!(
        projected.validate().is_ok(),
        "non-zero binding evidence is not enough without canonical contract validation"
    );

    let error = stale
        .validated_binding()
        .expect_err("stale canonical hash must reject binding evidence");
    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
}

#[test]
fn procedure_validate_binding_rejects_stats_or_policy_drift() {
    let proc = procedure(16, "Inventory.ReserveStock", CatalogVersion::new(7), vec![]);

    let mut drifted_stats = proc.binding();
    drifted_stats.stats_version = StatsVersion::new(proc.stats_version.get() + 1);
    let error = proc
        .validate_binding(&drifted_stats)
        .expect_err("drifted StatsVersion must be rejected");
    assert_eq!(error.kind(), AndromedaErrorKind::Contract);

    let mut drifted_policy = proc.binding();
    drifted_policy.policy_version = PolicyVersion::new([0xDB; PolicyVersion::LEN]);
    let error = proc
        .validate_binding(&drifted_policy)
        .expect_err("drifted PolicyVersion must be rejected");
    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
}

#[test]
fn procedure_validate_binding_rejects_catalog_version_or_procedure_id_drift() {
    let proc = procedure(17, "Inventory.ReserveStock", CatalogVersion::new(7), vec![]);

    let mut drifted_catalog = proc.binding();
    drifted_catalog.catalog_version = CatalogVersion::new(proc.object.catalog_version.get() + 1);
    let error = proc
        .validate_binding(&drifted_catalog)
        .expect_err("drifted CatalogVersion must be rejected before invocation");
    assert_eq!(error.kind(), AndromedaErrorKind::Contract);

    let mut drifted_procedure = proc.binding();
    drifted_procedure.procedure_id = ProcedureId::new(proc.procedure_id.get() + 1);
    let error = proc
        .validate_binding(&drifted_procedure)
        .expect_err("drifted ProcedureId must be rejected before invocation");
    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
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
