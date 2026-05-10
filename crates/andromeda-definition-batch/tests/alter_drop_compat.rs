#![forbid(unsafe_code)]

use andromeda_catalog_store::{CatalogDefinition, CatalogObjectRef, ObjectKind, QualifiedName};
use andromeda_definition_batch::{
    DefinitionBatch, DefinitionBatchDryRun, DefinitionBatchId, DefinitionOperation,
    dry_run_definition_batch,
};
use andromeda_procedure_contract::{
    AccessMode, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy, ProcedureContract,
    ProcedureContractCandidate, ProcedureErrorPolicy, ProtocolLayoutRef, ResultMetadataPolicy,
    StatsVersion, TransactionPolicy,
};
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, DatabaseId, NamespaceId,
    ProcedureId, ScalarType, TypeDescriptor,
};

const TEST_DB_ID: DatabaseId = DatabaseId::new(1);
const TEST_NS_ID: NamespaceId = NamespaceId::new(1);
const TEST_BATCH_ID_BASE: u64 = 1000;

fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(ScalarType::I64),
        ordinal,
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
        result_streams: vec![],
        required_permissions: vec!["test.Execute".to_string()],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Serializable,
            retryable: false,
        },
        compatibility_policy: CompatibilityPolicy::ExactHash,
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: vec![],
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
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
