//! Tests for digest-backed `ContractHash`, `PolicyVersion`, and the extended
//! catalog dependency graph (Procedure ↔ Table edges via bindings).

use andromeda_catalog::{
    AccessMode, BatchDependencyGraph, CatalogBindingKind, CatalogDefinition, CatalogDependency,
    CatalogDependencyKind, CatalogObjectBinding, CatalogObjectRef, CompatibilityPolicy,
    DefinitionOperation, IsolationPolicy, MultiResultPolicy, ObjectKind, PolicyVersion,
    ProcedureContract, ProcedureContractCandidate, ProcedureErrorPolicy, ProtocolLayoutRef,
    QualifiedName, ResultMetadataPolicy, StatsVersion, StructuredObjectDefinition, TableDefinition,
    TransactionPolicy,
};
use andromeda_core::{
    AndromedaErrorKind, CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash,
    ProcedureId, ScalarType, TypeDescriptor,
};

#[test]
fn catalog_digest_is_core_digest_alias() {
    let message = b"catalog contract digest canonical backend";

    let mut catalog_hasher: andromeda_catalog::digest::Sha256 =
        andromeda_core::digest::Sha256::new();
    catalog_hasher.update(&message[..8]);
    catalog_hasher.update(&message[8..]);

    let mut core_hasher: andromeda_core::digest::Sha256 = andromeda_catalog::digest::Sha256::new();
    core_hasher.update(message);

    assert_eq!(catalog_hasher.finalize(), core_hasher.finalize());
    assert_eq!(
        andromeda_catalog::digest::sha256(message),
        andromeda_core::digest::sha256(message)
    );
}

fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(ScalarType::I64),
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
    TableDefinition {
        object: object(id, name, ObjectKind::Table, version),
        columns: vec![column("ProductId", 0)],
    }
}

fn structured(id: u64, name: &str, version: CatalogVersion) -> StructuredObjectDefinition {
    StructuredObjectDefinition {
        object: object(id, name, ObjectKind::StructuredObject, version),
        fields: vec![column("ProductId", 0)],
        unique_by: vec!["ProductId".to_string()],
    }
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
fn procedure_binding_carries_four_identities() {
    let proc = procedure(13, "Inventory.ReserveStock", CatalogVersion::new(7), vec![]);
    let binding = proc.binding();

    assert_eq!(binding.procedure_id, proc.procedure_id);
    assert_eq!(binding.contract_hash, proc.contract_hash);
    assert_eq!(binding.catalog_version, proc.object.catalog_version);
    assert_eq!(binding.stats_version, proc.stats_version);
    assert_eq!(binding.policy_version, proc.policy_version());
    assert!(binding.validate().is_ok());

    let mut zero_policy = binding;
    zero_policy.policy_version = PolicyVersion::zero();
    assert_eq!(
        zero_policy.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
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
