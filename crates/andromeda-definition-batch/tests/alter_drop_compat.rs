#![forbid(unsafe_code)]

use andromeda_catalog_store::{
    CatalogDefinition, CatalogMutationDelta, CatalogObjectRef, CatalogPublicationSemantics,
    CatalogSnapshot, CatalogSnapshotMutationPlan, CatalogSnapshotReceipt, ObjectKind,
    QualifiedName,
};
use andromeda_definition_batch::{
    CatalogLifecycleAction, CatalogLifecycleTarget, DefinitionBatch, DefinitionBatchDryRun,
    DefinitionBatchId, DefinitionOperation, dry_run_definition_batch,
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

const TEST_DB_ID: DatabaseId = DatabaseId::new(1);
const TEST_NS_ID: NamespaceId = NamespaceId::new(1);
const TEST_BATCH_ID_BASE: u64 = 1000;

fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
    typed_column(name, ordinal, ScalarType::I64)
}

fn typed_column(name: &str, ordinal: u32, scalar_type: ScalarType) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(scalar_type),
        ordinal,
    }
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

fn object_ref(id: u64, name: &str, kind: ObjectKind, version: CatalogVersion) -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(id),
        name: QualifiedName::parse(name).expect("valid qualified name"),
        kind,
        catalog_version: version,
    }
}

fn procedure_contract(id: u64, name: &str, version: CatalogVersion) -> ProcedureContract {
    procedure_contract_with_structured_inputs(id, name, version, Vec::new())
}

fn procedure_contract_with_structured_inputs(
    id: u64,
    name: &str,
    version: CatalogVersion,
    structured_inputs: Vec<QualifiedName>,
) -> ProcedureContract {
    procedure_contract_with_shape(
        id,
        name,
        version,
        structured_inputs,
        Vec::new(),
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::SingleResultOnly,
    )
}

fn procedure_contract_with_shape(
    id: u64,
    name: &str,
    version: CatalogVersion,
    structured_inputs: Vec<QualifiedName>,
    result_streams: Vec<ResultStreamContract>,
    compatibility_policy: CompatibilityPolicy,
    multi_result_policy: MultiResultPolicy,
) -> ProcedureContract {
    ProcedureContractCandidate {
        object: object_ref(id, name, ObjectKind::Procedure, version),
        procedure_id: ProcedureId::new(id),
        stats_version: StatsVersion::new(1),
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(0xA1),
            frame_envelope_hash: ContractHash::test_vector(0xA2),
        },
        inputs: vec![column("input_param", 0)],
        structured_inputs,
        result_streams,
        required_permissions: vec!["test.Execute".to_string()],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Serializable,
            retryable: false,
        },
        compatibility_policy,
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: vec![],
        },
        multi_result_policy,
    }
    .materialize()
    .expect("valid contract")
}

fn create_procedure_operation(
    procedure_id: u64,
    name: &str,
    next_version: CatalogVersion,
) -> DefinitionOperation {
    DefinitionOperation::Create(CatalogDefinition::Procedure(procedure_contract(
        procedure_id,
        name,
        next_version,
    )))
}

fn definition_batch(
    base_version: CatalogVersion,
    operations: Vec<DefinitionOperation>,
) -> DefinitionBatch {
    DefinitionBatch {
        batch_id: DefinitionBatchId::new(TEST_BATCH_ID_BASE + base_version.get()),
        database_id: TEST_DB_ID,
        namespace_id: TEST_NS_ID,
        base_version,
        operations,
    }
}

fn batch_with_create(
    base_version: CatalogVersion,
    procedure_id: u64,
    name: &str,
    next_version: CatalogVersion,
) -> DefinitionBatch {
    definition_batch(
        base_version,
        vec![create_procedure_operation(procedure_id, name, next_version)],
    )
}

fn dry_run(batch: &DefinitionBatch) -> DefinitionBatchDryRun {
    dry_run_definition_batch(
        batch.batch_id,
        batch.database_id,
        batch.namespace_id,
        batch.base_version,
        &batch.operations,
    )
    .expect("valid definition batch dry-run")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TestReceipt {
    next_version: CatalogVersion,
}

impl CatalogSnapshotReceipt for TestReceipt {
    fn next_version(&self) -> CatalogVersion {
        self.next_version
    }
}

#[derive(Debug)]
struct TestApplyPlan {
    database_id: DatabaseId,
    namespace_id: NamespaceId,
    previous_version: CatalogVersion,
    next_version: CatalogVersion,
    deltas: Vec<CatalogMutationDelta<CatalogLifecycleTarget>>,
}

impl CatalogSnapshotMutationPlan for TestApplyPlan {
    type LifecycleTarget = CatalogLifecycleTarget;

    fn database_id(&self) -> DatabaseId {
        self.database_id
    }

    fn namespace_id(&self) -> NamespaceId {
        self.namespace_id
    }

    fn previous_version(&self) -> CatalogVersion {
        self.previous_version
    }

    fn next_version(&self) -> CatalogVersion {
        self.next_version
    }

    fn publication_semantics(&self) -> CatalogPublicationSemantics {
        CatalogPublicationSemantics::DurablePublicationExternal
    }

    fn is_monotonic(&self) -> bool {
        self.next_version.get() > self.previous_version.get()
    }

    fn deltas(&self) -> &[CatalogMutationDelta<Self::LifecycleTarget>] {
        &self.deltas
    }
}

fn apply_plan_from_batch(batch: &DefinitionBatch) -> TestApplyPlan {
    let plan = dry_run(batch);
    let deltas = batch
        .operations
        .iter()
        .enumerate()
        .map(|(operation_index, operation)| match operation {
            DefinitionOperation::Create(definition) => {
                CatalogMutationDelta::create(operation_index, plan.next_version, definition.clone())
            },
            DefinitionOperation::Deprecate(target) => {
                CatalogMutationDelta::deprecate(operation_index, plan.next_version, target.clone())
            },
        })
        .collect();

    TestApplyPlan {
        database_id: batch.database_id,
        namespace_id: batch.namespace_id,
        previous_version: plan.previous_version,
        next_version: plan.next_version,
        deltas,
    }
}

fn assert_has_compat_message(
    contract: &ProcedureContract,
    previous: &ProcedureContract,
    text: &str,
) {
    let diagnostic = contract.compatibility_with(previous);
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
fn create_procedure_generates_planned_version_and_contract_hash() {
    let base_version = CatalogVersion::new(1);
    let next_version = CatalogVersion::new(2);
    let batch = batch_with_create(base_version, 100, "test.Proc1", next_version);

    let plan = dry_run(&batch);

    assert_eq!(plan.previous_version, base_version);
    assert_eq!(plan.next_version, next_version);
    assert_eq!(plan.created_objects.len(), 1);
    let created = &plan.created_objects[0];
    assert_eq!(created.object_id, CatalogObjectId::new(100));
    assert_eq!(created.name, QualifiedName::parse("test.Proc1").unwrap());
    assert_eq!(created.kind, ObjectKind::Procedure);
    assert_eq!(created.planned_version, next_version);
    assert!(plan.deprecated_objects.is_empty());

    let DefinitionOperation::Create(CatalogDefinition::Procedure(contract)) = &batch.operations[0]
    else {
        panic!("expected procedure create operation");
    };
    assert_eq!(contract.object.catalog_version, next_version);
    assert_eq!(contract.contract_hash, contract.canonical_hash());
    assert!(!contract.contract_hash.is_zero());
}

#[test]
fn alter_compatible_procedure_shape_preserves_identity_and_advances_version() {
    let base_v1 = CatalogVersion::new(1);
    let v2 = CatalogVersion::new(2);

    let batch1 = batch_with_create(base_v1, 100, "test.ProcX", v2);
    let plan1 = dry_run(&batch1);
    let DefinitionOperation::Create(CatalogDefinition::Procedure(original_contract)) =
        &batch1.operations[0]
    else {
        panic!("expected procedure");
    };

    let v3 = CatalogVersion::new(3);
    let batch2 = batch_with_create(v2, 100, "test.ProcX", v3);
    let plan2 = dry_run(&batch2);
    let DefinitionOperation::Create(CatalogDefinition::Procedure(new_contract)) =
        &batch2.operations[0]
    else {
        panic!("expected procedure");
    };

    assert_eq!(plan1.next_version, v2);
    assert_eq!(plan2.previous_version, v2);
    assert_eq!(plan2.next_version, v3);
    assert_eq!(new_contract.procedure_id, original_contract.procedure_id);
    assert_eq!(new_contract.procedure_id, ProcedureId::new(100));
    assert_eq!(new_contract.object.name, original_contract.object.name);
    assert_eq!(
        new_contract.object.object_id,
        original_contract.object.object_id
    );
    assert_eq!(new_contract.contract_hash, original_contract.contract_hash);
    assert_eq!(plan2.next_version.get() - plan1.next_version.get(), 1);
}

#[test]
fn drop_compatibility_uses_dependency_bearing_procedure_shape() {
    let base_v1 = CatalogVersion::new(1);
    let v2 = CatalogVersion::new(2);
    let plan1 = dry_run(&batch_with_create(base_v1, 101, "test.DropMe", v2));

    let v3 = CatalogVersion::new(3);
    let drop_dependency = QualifiedName::parse("test.DropMe").unwrap();
    let dependent_contract = procedure_contract_with_structured_inputs(
        102,
        "test.DepOnDropMe",
        v3,
        vec![drop_dependency.clone()],
    );
    let batch2 = definition_batch(
        v2,
        vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
            dependent_contract,
        ))],
    );
    let plan2 = dry_run(&batch2);

    assert_eq!(plan1.created_objects.len(), 1);
    assert_eq!(plan2.created_objects.len(), 1);
    assert_eq!(plan2.next_version, v3);
    assert_eq!(
        batch2.dependency_graph_hash().unwrap(),
        plan2.dependency_graph_hash
    );
    assert!(!plan2.dependency_graph_hash.is_zero());
}

#[test]
fn additive_policy_accepts_append_only_result_growth_in_definition_batch() {
    let previous = procedure_contract_with_shape(
        120,
        "test.Additive",
        CatalogVersion::new(2),
        Vec::new(),
        vec![result_stream(
            1,
            "Rows",
            ResultStreamCardinality::Many,
            vec![column("id", 0)],
        )],
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );
    let additive = procedure_contract_with_shape(
        120,
        "test.Additive",
        CatalogVersion::new(3),
        Vec::new(),
        vec![
            result_stream(
                1,
                "Rows",
                ResultStreamCardinality::Many,
                vec![column("id", 0), column("quantity", 1)],
            ),
            result_stream(
                2,
                "Audit",
                ResultStreamCardinality::Many,
                vec![column("audit_id", 0)],
            ),
        ],
        CompatibilityPolicy::AdditiveOnly,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );
    let batch = definition_batch(
        CatalogVersion::new(2),
        vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
            additive.clone(),
        ))],
    );

    let plan = dry_run(&batch);
    let diagnostic = additive.compatibility_with(&previous);

    assert!(diagnostic.compatible, "{:?}", diagnostic.messages);
    assert_eq!(plan.previous_version, CatalogVersion::new(2));
    assert_eq!(plan.next_version, CatalogVersion::new(3));
    assert_eq!(plan.created_objects.len(), 1);
    assert_ne!(
        previous.contract_hash, additive.contract_hash,
        "append-only result growth is compatible but still changes ContractHash"
    );
}

#[test]
fn breaking_policy_rejects_shape_shift_even_when_dry_run_can_plan_current_contract() {
    let previous = procedure_contract_with_shape(
        121,
        "test.Breaking",
        CatalogVersion::new(2),
        Vec::new(),
        vec![result_stream(
            1,
            "Rows",
            ResultStreamCardinality::Many,
            vec![column("id", 0), column("quantity", 1)],
        )],
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );
    let breaking = procedure_contract_with_shape(
        121,
        "test.Breaking",
        CatalogVersion::new(3),
        Vec::new(),
        vec![result_stream(
            1,
            "Rows",
            ResultStreamCardinality::One,
            vec![
                typed_column("id", 0, ScalarType::Bool),
                column("quantity", 1),
            ],
        )],
        CompatibilityPolicy::AdditiveOnly,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );
    let batch = definition_batch(
        CatalogVersion::new(2),
        vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
            breaking.clone(),
        ))],
    );

    let plan = dry_run(&batch);
    let diagnostic = breaking.compatibility_with(&previous);

    assert_eq!(plan.next_version, CatalogVersion::new(3));
    assert!(
        !diagnostic.compatible,
        "contract-breaking changes must be rejected by compatibility policy"
    );
    assert_has_compat_message(&breaking, &previous, "changing cardinality contract");
    assert_has_compat_message(
        &breaking,
        &previous,
        "existing columns to remain an unchanged prefix",
    );
}

#[test]
fn deprecate_operation_records_ordered_deprecated_lifecycle_evidence() {
    let base_version = CatalogVersion::new(7);
    let target = object_ref(122, "test.Deprecated", ObjectKind::Procedure, base_version);
    let batch = definition_batch(
        base_version,
        vec![DefinitionOperation::Deprecate(CatalogLifecycleTarget {
            object: target.clone(),
        })],
    );

    let plan = dry_run(&batch);

    assert!(plan.created_objects.is_empty());
    assert_eq!(plan.deprecated_objects.len(), 1);
    let deprecated = &plan.deprecated_objects[0];
    assert_eq!(deprecated.object_id, target.object_id);
    assert_eq!(deprecated.name, target.name);
    assert_eq!(deprecated.kind, ObjectKind::Procedure);
    assert_eq!(deprecated.action, CatalogLifecycleAction::Deprecate);
    assert_eq!(deprecated.planned_version, CatalogVersion::new(8));
}

#[test]
fn dry_run_rejects_stale_canonical_contract_hash() {
    let mut stale = procedure_contract(123, "test.Rejected", CatalogVersion::new(2));
    stale.contract_hash = ContractHash::test_vector(0xCC);
    let batch = definition_batch(
        CatalogVersion::new(1),
        vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
            stale,
        ))],
    );

    let error = dry_run_definition_batch(
        batch.batch_id,
        batch.database_id,
        batch.namespace_id,
        batch.base_version,
        &batch.operations,
    )
    .expect_err("stale canonical contract hash must reject dry-run");

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(
        error
            .message()
            .contains("procedure contract hash must match canonical contract shape"),
        "{}",
        error.message()
    );
}

#[test]
fn apply_rejects_stale_base_catalog_without_advancing_snapshot() {
    let mut snapshot: CatalogSnapshot<TestReceipt> =
        CatalogSnapshot::empty(TEST_DB_ID, TEST_NS_ID, CatalogVersion::new(2));
    let stale_batch = batch_with_create(
        CatalogVersion::new(1),
        124,
        "test.StaleBase",
        CatalogVersion::new(2),
    );
    let plan = apply_plan_from_batch(&stale_batch);

    let error = snapshot
        .apply_mutation_plan(&plan)
        .expect_err("stale base catalog version must be rejected");

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(
        error
            .message()
            .contains("previous version must match snapshot version"),
        "{}",
        error.message()
    );
    assert_eq!(snapshot.version, CatalogVersion::new(2));
    assert_eq!(snapshot.applied_object_count(), 0);
}

#[test]
fn apply_rejects_duplicate_and_conflicting_recreate_attempts() {
    let mut snapshot: CatalogSnapshot<TestReceipt> =
        CatalogSnapshot::empty(TEST_DB_ID, TEST_NS_ID, CatalogVersion::new(1));
    let create_batch = batch_with_create(
        CatalogVersion::new(1),
        125,
        "test.ApplyOnce",
        CatalogVersion::new(2),
    );
    let create_plan = apply_plan_from_batch(&create_batch);
    snapshot.apply_mutation_plan(&create_plan).unwrap();

    let duplicate_apply = snapshot
        .apply_mutation_plan(&create_plan)
        .expect_err("same apply plan cannot be replayed against an advanced snapshot");
    assert_eq!(duplicate_apply.kind(), AndromedaErrorKind::Catalog);
    assert!(
        duplicate_apply
            .message()
            .contains("previous version must match snapshot version"),
        "{}",
        duplicate_apply.message()
    );

    let conflicting_recreate = procedure_contract_with_shape(
        125,
        "test.ApplyOnce",
        CatalogVersion::new(3),
        Vec::new(),
        vec![result_stream(
            1,
            "Rows",
            ResultStreamCardinality::One,
            vec![column("id", 0)],
        )],
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::SingleResultOnly,
    );
    let conflicting_batch = definition_batch(
        CatalogVersion::new(2),
        vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
            conflicting_recreate,
        ))],
    );
    let conflicting_plan = apply_plan_from_batch(&conflicting_batch);
    let conflict = snapshot
        .apply_mutation_plan(&conflicting_plan)
        .expect_err("create cannot be used as an implicit alter over an existing object");

    assert_eq!(conflict.kind(), AndromedaErrorKind::Catalog);
    assert!(
        conflict.message().contains("already contains object id"),
        "{}",
        conflict.message()
    );
    assert_eq!(snapshot.version, CatalogVersion::new(2));
    assert_eq!(snapshot.applied_object_count(), 1);
}

#[test]
fn definition_batch_dry_run_is_idempotent() {
    let base_version = CatalogVersion::new(1);
    let batch = batch_with_create(base_version, 200, "test.Idempotent", CatalogVersion::new(2));

    let plan1 = dry_run(&batch);
    let plan2 = dry_run(&batch);

    assert_eq!(plan1, plan2);
    assert_eq!(plan1.source_hash, batch.source_hash());
    assert_eq!(
        plan1.dependency_graph_hash,
        batch.dependency_graph_hash().unwrap()
    );
}

#[test]
fn definition_batch_version_advancement_is_monotonic() {
    let base_version = CatalogVersion::new(5);
    let plan = dry_run(&batch_with_create(
        base_version,
        400,
        "test.Monotonic",
        CatalogVersion::new(6),
    ));

    assert_eq!(plan.next_version.get() - plan.previous_version.get(), 1);
    assert!(plan.next_version.get() > plan.previous_version.get());
}
