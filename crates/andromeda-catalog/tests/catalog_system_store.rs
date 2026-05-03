use andromeda_catalog::{
    AccessMode, CatalogDefinition, CatalogMutationRecordKind, CatalogObjectRef,
    CatalogPublicationSemantics, CatalogSnapshotPublication, CatalogSystemStore,
    CompatibilityPolicy, DefinitionBatch, DefinitionBatchId, DefinitionOperation, IsolationPolicy,
    MultiResultPolicy, ObjectKind, ProcedureContract, ProcedureErrorPolicy, ProtocolLayoutRef,
    QualifiedName, ResultMetadataPolicy, StatsVersion, StructuredObjectDefinition, TableDefinition,
    TransactionPolicy,
};
use andromeda_core::{
    AndromedaErrorKind, CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash,
    DatabaseId, NamespaceId, ProcedureId, ScalarType, TypeDescriptor,
};

const DATABASE_ID: DatabaseId = DatabaseId::new(1);
const NAMESPACE_ID: NamespaceId = NamespaceId::new(2);

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

fn structured_object(id: u64, name: &str, version: CatalogVersion) -> StructuredObjectDefinition {
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
    ProcedureContract {
        object: object(id, name, ObjectKind::Procedure, version),
        procedure_id: ProcedureId::new(id),
        contract_hash: ContractHash::test_vector(id as u8),
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
fn mutation_records_are_ordered_begin_apply_commit() {
    let store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    let plan = store
        .plan_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![
                DefinitionOperation::Create(CatalogDefinition::Table(table(
                    1,
                    "Inventory.Product",
                    CatalogVersion::new(11),
                ))),
                DefinitionOperation::Create(CatalogDefinition::Table(table(
                    2,
                    "Inventory.Stock",
                    CatalogVersion::new(11),
                ))),
            ],
        ))
        .unwrap();

    let record_kinds: Vec<CatalogMutationRecordKind> = plan
        .mutation_plan
        .records()
        .iter()
        .map(|record| record.kind())
        .collect();

    assert_eq!(
        record_kinds,
        vec![
            CatalogMutationRecordKind::CatalogChangeBegin,
            CatalogMutationRecordKind::CatalogChangeApply,
            CatalogMutationRecordKind::CatalogChangeApply,
            CatalogMutationRecordKind::CatalogChangeCommit,
        ]
    );
}

#[test]
fn snapshot_apply_advances_version_without_claiming_durable_publication() {
    let mut store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    let report = store
        .apply_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Create(CatalogDefinition::Table(
                table(1, "Inventory.Product", CatalogVersion::new(11)),
            ))],
        ))
        .unwrap();

    assert_eq!(store.snapshot().version, CatalogVersion::new(11));
    assert_eq!(
        report.snapshot_report.previous_version,
        CatalogVersion::new(10)
    );
    assert_eq!(report.snapshot_report.next_version, CatalogVersion::new(11));
    assert_eq!(
        report.snapshot_report.publication_semantics,
        CatalogPublicationSemantics::DurablePublicationExternal
    );
    assert!(!report.snapshot_report.durable_publication_performed);
    assert_eq!(
        store.snapshot().publication,
        CatalogSnapshotPublication::InMemoryOnly
    );
}

#[test]
fn duplicate_object_ids_and_names_are_rejected() {
    let store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));

    let duplicate_id_error = store
        .plan_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![
                DefinitionOperation::Create(CatalogDefinition::Table(table(
                    1,
                    "Inventory.Product",
                    CatalogVersion::new(11),
                ))),
                DefinitionOperation::Create(CatalogDefinition::Table(table(
                    1,
                    "Inventory.Stock",
                    CatalogVersion::new(11),
                ))),
            ],
        ))
        .unwrap_err();
    assert_eq!(duplicate_id_error.kind(), AndromedaErrorKind::Catalog);
    assert!(duplicate_id_error.message().contains("object id"));

    let duplicate_name_error = store
        .plan_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![
                DefinitionOperation::Create(CatalogDefinition::Table(table(
                    1,
                    "Inventory.Product",
                    CatalogVersion::new(11),
                ))),
                DefinitionOperation::Create(CatalogDefinition::Table(table(
                    2,
                    "Inventory.Product",
                    CatalogVersion::new(11),
                ))),
            ],
        ))
        .unwrap_err();
    assert_eq!(duplicate_name_error.kind(), AndromedaErrorKind::Catalog);
    assert!(duplicate_name_error.message().contains("object name"));
}

#[test]
fn dependency_ordering_is_rejected_for_late_structured_inputs() {
    let store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));

    let error = store
        .plan_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![
                DefinitionOperation::Create(CatalogDefinition::Procedure(procedure(
                    2,
                    "Inventory.ReserveStock",
                    CatalogVersion::new(11),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ))),
                DefinitionOperation::Create(CatalogDefinition::StructuredObject(
                    structured_object(1, "Inventory.StockRequest", CatalogVersion::new(11)),
                )),
            ],
        ))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("dependencies before dependent"));
}

#[test]
fn catalog_api_has_no_storage_or_exec_coupling() {
    let manifest = include_str!("../Cargo.toml");

    assert!(!manifest.contains("andromeda-storage"));
    assert!(!manifest.contains("andromeda-exec"));

    let store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(1));
    assert_eq!(
        store.snapshot().publication,
        CatalogSnapshotPublication::InMemoryOnly
    );
}
