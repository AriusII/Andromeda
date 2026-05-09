pub(crate) use andromeda_catalog::{
    AccessMode, CatalogDefinition, CatalogDefinitionBatchPlanning, CatalogObjectRef,
    CompatibilityPolicy, DefinitionBatch, DefinitionBatchId, DefinitionOperation, IsolationPolicy,
    MultiResultPolicy, ObjectKind, ProcedureContract, ProcedureContractCandidate,
    ProcedureErrorPolicy, ProtocolLayoutRef, QualifiedName, ResultMetadataPolicy, StatsVersion,
    TransactionPolicy,
};
pub(crate) use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, DatabaseId, NamespaceId,
    ProcedureId, ScalarType, TypeDescriptor,
};

pub(crate) const TEST_DB_ID: DatabaseId = DatabaseId::new(1);
pub(crate) const TEST_NS_ID: NamespaceId = NamespaceId::new(1);
pub(crate) const TEST_BATCH_ID_BASE: u64 = 1000;

pub(crate) fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(ScalarType::I64),
        ordinal,
    }
}

pub(crate) fn object_ref(
    id: u64,
    name: &str,
    kind: ObjectKind,
    version: CatalogVersion,
) -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(id),
        name: QualifiedName::parse(name).expect("valid qualified name"),
        kind,
        catalog_version: version,
    }
}

pub(crate) fn procedure_contract(
    id: u64,
    name: &str,
    version: CatalogVersion,
) -> ProcedureContract {
    procedure_contract_with_structured_inputs(id, name, version, Vec::new())
}

pub(crate) fn procedure_contract_with_structured_inputs(
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

pub(crate) fn create_procedure_operation(
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

pub(crate) fn definition_batch(
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

pub(crate) fn batch_with_create(
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
