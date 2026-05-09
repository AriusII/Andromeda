pub(crate) use andromeda_catalog::{
    AccessMode, CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH, CatalogDefinition,
    CatalogDependencyKind, CatalogDurabilityMarker, CatalogDurableMutationPayload,
    CatalogLifecycleTarget, CatalogMutationCommitEvidence, CatalogMutationDurability,
    CatalogMutationOperation, CatalogMutationPlan, CatalogMutationRecord,
    CatalogMutationRecordKind, CatalogObjectRef, CatalogPublicationSemantics,
    CatalogRecoveryAnomalyKind, CatalogRecoveryOutcome, CatalogSkippedBatchReason,
    CatalogSnapshotPublication, CatalogSystemStore, CompatibilityPolicy, DefinitionBatch,
    DefinitionBatchId, DefinitionBatchPlan, DefinitionBatchSourceHash, DefinitionOperation,
    IsolationPolicy, MultiResultPolicy, ObjectKind, ProcedureContract, ProcedureContractCandidate,
    ProcedureErrorPolicy, ProtocolLayoutRef, QualifiedName, ResultMetadataPolicy, StatsVersion,
    StructuredObjectDefinition, TableDefinition, TransactionPolicy,
    recover_catalog_snapshot_from_durable_payloads, replay_catalog_mutation_records,
};
pub(crate) use andromeda_error::{AndromedaError, AndromedaErrorKind};
pub(crate) use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, DatabaseId, NamespaceId,
    ProcedureId, ScalarType, TransactionId, TypeDescriptor,
};
pub(crate) use andromeda_wal::{
    Lsn, WalRecord, WalRecordKind, decode_wal_record_frame, encode_wal_record,
};

pub(crate) const DATABASE_ID: DatabaseId = DatabaseId::new(1);
pub(crate) const NAMESPACE_ID: NamespaceId = NamespaceId::new(2);

pub(crate) fn version(value: u64) -> CatalogVersion {
    CatalogVersion::new(value)
}

pub(crate) fn store_at(catalog_version: u64) -> CatalogSystemStore {
    CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, version(catalog_version))
}

pub(crate) fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(ScalarType::I64),
        ordinal,
    }
}

pub(crate) fn object(
    id: u64,
    name: &str,
    kind: ObjectKind,
    version: CatalogVersion,
) -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(id),
        name: QualifiedName::parse(name).unwrap(),
        kind,
        catalog_version: version,
    }
}

pub(crate) fn table(id: u64, name: &str, version: CatalogVersion) -> TableDefinition {
    TableDefinition {
        object: object(id, name, ObjectKind::Table, version),
        columns: vec![column("ProductId", 0)],
    }
}

pub(crate) fn create_table(id: u64, name: &str, catalog_version: u64) -> DefinitionOperation {
    DefinitionOperation::Create(CatalogDefinition::Table(table(
        id,
        name,
        version(catalog_version),
    )))
}

pub(crate) fn create_inventory_product(catalog_version: u64) -> DefinitionOperation {
    create_table(1, "Inventory.Product", catalog_version)
}

pub(crate) fn structured_object(
    id: u64,
    name: &str,
    version: CatalogVersion,
) -> StructuredObjectDefinition {
    StructuredObjectDefinition {
        object: object(id, name, ObjectKind::StructuredObject, version),
        fields: vec![column("ProductId", 0)],
        unique_by: vec!["ProductId".to_string()],
    }
}

pub(crate) fn procedure(
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

pub(crate) fn batch(
    base_version: CatalogVersion,
    operations: Vec<DefinitionOperation>,
) -> DefinitionBatch {
    DefinitionBatch {
        batch_id: DefinitionBatchId::new(base_version.get() + 100),
        database_id: DATABASE_ID,
        namespace_id: NAMESPACE_ID,
        base_version,
        operations,
    }
}

pub(crate) fn product_batch(base_version: u64, next_version: u64) -> DefinitionBatch {
    batch(
        version(base_version),
        vec![create_inventory_product(next_version)],
    )
}

pub(crate) fn plan_product_batch(
    store: &CatalogSystemStore,
    base_version: u64,
    next_version: u64,
) -> DefinitionBatchPlan {
    store
        .plan_definition_batch(&product_batch(base_version, next_version))
        .unwrap()
}

pub(crate) fn replay_records_at(
    catalog_version: u64,
    records: Vec<CatalogMutationRecord>,
) -> CatalogRecoveryOutcome {
    replay_catalog_mutation_records(store_at(catalog_version).into_snapshot(), records)
}

pub(crate) fn recover_payloads_at<'a>(
    catalog_version: u64,
    payloads: impl IntoIterator<Item = CatalogDurableMutationPayload<'a>>,
) -> CatalogRecoveryOutcome {
    recover_catalog_snapshot_from_durable_payloads(
        store_at(catalog_version).into_snapshot(),
        payloads,
    )
}

pub(crate) fn assert_has_anomaly(
    outcome: &CatalogRecoveryOutcome,
    kind: CatalogRecoveryAnomalyKind,
) {
    assert!(
        outcome
            .report
            .anomalies
            .iter()
            .any(|anomaly| anomaly.kind == kind)
    );
}

pub(crate) fn assert_empty_snapshot_at(outcome: &CatalogRecoveryOutcome, catalog_version: u64) {
    assert_eq!(outcome.snapshot.version, version(catalog_version));
    assert_eq!(outcome.snapshot.object_count(), 0);
}

pub(crate) fn assert_store_unpublished_at(store: &CatalogSystemStore, catalog_version: u64) {
    assert_eq!(store.snapshot().version, version(catalog_version));
    assert_eq!(
        store.snapshot().publication,
        CatalogSnapshotPublication::InMemoryOnly
    );
}
