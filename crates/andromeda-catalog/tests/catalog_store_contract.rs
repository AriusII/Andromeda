use andromeda_catalog::{
    AccessMode, CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH, CatalogDefinition,
    CatalogDependencyKind, CatalogDurabilityMarker, CatalogDurableMutationPayload,
    CatalogLifecycleTarget, CatalogMutationCommitEvidence, CatalogMutationDurability,
    CatalogMutationOperation, CatalogMutationRecord, CatalogMutationRecordKind, CatalogObjectRef,
    CatalogPublicationSemantics, CatalogRecoveryAnomalyKind, CatalogRecoveryOutcome,
    CatalogSkippedBatchReason, CatalogSnapshotPublication, CatalogSystemStore, CompatibilityPolicy,
    DefinitionBatch, DefinitionBatchId, DefinitionBatchPlan, DefinitionBatchSourceHash,
    DefinitionOperation, IsolationPolicy, MultiResultPolicy, ObjectKind, ProcedureContract,
    ProcedureContractCandidate, ProcedureErrorPolicy, ProtocolLayoutRef, QualifiedName,
    ResultMetadataPolicy, StatsVersion, StructuredObjectDefinition, TableDefinition,
    TransactionPolicy, recover_catalog_snapshot_from_durable_payloads,
    replay_catalog_mutation_records,
};
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, CatalogObjectId, CatalogVersion, ColumnDescriptor,
    ContractHash, DatabaseId, NamespaceId, ProcedureId, ScalarType, TransactionId, TypeDescriptor,
};
use andromeda_storage::{
    Lsn, WalRecord, WalRecordKind, decode_wal_record_frame, encode_wal_record,
};

const DATABASE_ID: DatabaseId = DatabaseId::new(1);
const NAMESPACE_ID: NamespaceId = NamespaceId::new(2);

fn version(value: u64) -> CatalogVersion {
    CatalogVersion::new(value)
}

fn store_at(catalog_version: u64) -> CatalogSystemStore {
    CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, version(catalog_version))
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

fn create_table(id: u64, name: &str, catalog_version: u64) -> DefinitionOperation {
    DefinitionOperation::Create(CatalogDefinition::Table(table(
        id,
        name,
        version(catalog_version),
    )))
}

fn create_inventory_product(catalog_version: u64) -> DefinitionOperation {
    create_table(1, "Inventory.Product", catalog_version)
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

fn batch(base_version: CatalogVersion, operations: Vec<DefinitionOperation>) -> DefinitionBatch {
    DefinitionBatch {
        batch_id: DefinitionBatchId::new(base_version.get() + 100),
        database_id: DATABASE_ID,
        namespace_id: NAMESPACE_ID,
        base_version,
        operations,
    }
}

fn product_batch(base_version: u64, next_version: u64) -> DefinitionBatch {
    batch(
        version(base_version),
        vec![create_inventory_product(next_version)],
    )
}

fn plan_product_batch(
    store: &CatalogSystemStore,
    base_version: u64,
    next_version: u64,
) -> DefinitionBatchPlan {
    store
        .plan_definition_batch(&product_batch(base_version, next_version))
        .unwrap()
}

fn replay_records_at(
    catalog_version: u64,
    records: Vec<CatalogMutationRecord>,
) -> CatalogRecoveryOutcome {
    replay_catalog_mutation_records(store_at(catalog_version).into_snapshot(), records)
}

fn recover_payloads_at<'a>(
    catalog_version: u64,
    payloads: impl IntoIterator<Item = CatalogDurableMutationPayload<'a>>,
) -> CatalogRecoveryOutcome {
    recover_catalog_snapshot_from_durable_payloads(
        store_at(catalog_version).into_snapshot(),
        payloads,
    )
}

fn assert_has_anomaly(outcome: &CatalogRecoveryOutcome, kind: CatalogRecoveryAnomalyKind) {
    assert!(
        outcome
            .report
            .anomalies
            .iter()
            .any(|anomaly| anomaly.kind == kind)
    );
}

fn assert_empty_snapshot_at(outcome: &CatalogRecoveryOutcome, catalog_version: u64) {
    assert_eq!(outcome.snapshot.version, version(catalog_version));
    assert_eq!(outcome.snapshot.object_count(), 0);
}

fn assert_store_unpublished_at(store: &CatalogSystemStore, catalog_version: u64) {
    assert_eq!(store.snapshot().version, version(catalog_version));
    assert_eq!(
        store.snapshot().publication,
        CatalogSnapshotPublication::InMemoryOnly
    );
}

#[test]
fn mutation_records_are_ordered_begin_apply_commit() {
    let store = store_at(10);
    let plan = store
        .plan_definition_batch(&batch(
            version(10),
            vec![
                create_table(1, "Inventory.Product", 11),
                create_table(2, "Inventory.Stock", 11),
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
fn catalog_mutation_records_fit_storage_wal_catalog_kinds() {
    let store = store_at(10);
    let plan = plan_product_batch(&store, 10, 11);

    let mut previous_lsn = None;
    for (index, catalog_record) in plan.mutation_plan.records().into_iter().enumerate() {
        let storage_kind = match catalog_record.kind() {
            CatalogMutationRecordKind::CatalogChangeBegin => WalRecordKind::CatalogChangeBegin,
            CatalogMutationRecordKind::CatalogChangeApply => WalRecordKind::CatalogChangeApply,
            CatalogMutationRecordKind::CatalogChangeCommit => WalRecordKind::CatalogChangeCommit,
        };
        assert_eq!(
            catalog_record.kind().storage_wal_kind_tag() as u64,
            andromeda_storage::wal_record_kind_tag(storage_kind)
        );

        let payload = catalog_record.encode_durable_payload().unwrap();
        let wal_record = WalRecord::from_parts(
            storage_kind,
            Lsn::new((index + 1) as u64),
            previous_lsn,
            Some(TransactionId::new(77)),
            payload,
        )
        .unwrap();
        let encoded = encode_wal_record(&wal_record).unwrap();
        let (decoded_wal_record, consumed) = decode_wal_record_frame(&encoded).unwrap().unwrap();

        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded_wal_record.header.kind, storage_kind);
        assert_eq!(
            CatalogMutationRecord::decode_durable_payload(decoded_wal_record.payload()).unwrap(),
            catalog_record
        );
        previous_lsn = Some(wal_record.header.lsn);
    }
}

#[test]
fn snapshot_apply_advances_version_without_claiming_durable_publication() {
    let mut store = store_at(10);
    let report = store
        .apply_definition_batch(&product_batch(10, 11))
        .unwrap();

    assert_eq!(store.snapshot().version, version(11));
    assert_eq!(report.snapshot_report.previous_version, version(10));
    assert_eq!(report.snapshot_report.next_version, version(11));
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
fn durable_publication_advances_visible_snapshot_with_receipt() {
    let mut store = store_at(10);
    let plan = plan_product_batch(&store, 10, 11);
    let records = plan.mutation_plan.records();
    let evidence = CatalogMutationCommitEvidence::from_durable_commit_record(
        records.last().unwrap(),
        records.len(),
        CatalogMutationDurability::StorageWal {
            commit_lsn: Lsn::new(77).get(),
            durable_lsn: Lsn::new(80).get(),
        },
    )
    .unwrap();

    let receipt = store
        .publish_durable_mutation_plan(&plan.mutation_plan, evidence)
        .unwrap();

    assert_eq!(store.snapshot().version, version(11));
    assert_eq!(receipt.previous_version, version(10));
    assert_eq!(receipt.next_version, version(11));
    assert_eq!(receipt.batch_id, plan.batch_id);
    assert_eq!(receipt.durable_lsn, Some(80));
    assert_eq!(receipt.record_count, records.len());
    assert_eq!(
        receipt.publication_semantics,
        CatalogPublicationSemantics::DurablePublicationExternal
    );
    assert_eq!(
        store.snapshot().publication,
        CatalogSnapshotPublication::Durable(receipt)
    );
}

#[test]
fn durable_apply_writes_catalog_wal_before_visible_publication() {
    let mut store = store_at(10);
    let mut appended = Vec::new();
    let mut next_lsn = 70;
    let mut flush_target = None;

    let report = store
        .apply_definition_batch_durably(
            &product_batch(10, 11),
            |kind, payload| {
                let lsn = next_lsn;
                next_lsn += 1;
                appended.push((kind, payload.to_vec(), lsn));
                Ok(lsn)
            },
            |commit_lsn| {
                flush_target = Some(commit_lsn);
                Ok(commit_lsn)
            },
        )
        .unwrap();

    assert_eq!(appended.len(), 3);
    assert_eq!(
        appended
            .iter()
            .map(|(kind, _, _)| *kind)
            .collect::<Vec<_>>(),
        vec![
            CatalogMutationRecordKind::CatalogChangeBegin,
            CatalogMutationRecordKind::CatalogChangeApply,
            CatalogMutationRecordKind::CatalogChangeCommit,
        ]
    );
    assert_eq!(flush_target, Some(72));
    assert_eq!(report.receipt.durable_lsn, Some(72));
    assert_eq!(report.appended_records[2].lsn, 72);
    assert_eq!(store.snapshot().version, version(11));
    assert!(matches!(
        store.snapshot().publication,
        CatalogSnapshotPublication::Durable(receipt) if receipt == report.receipt
    ));
}

#[test]
fn durable_apply_rejects_non_monotonic_wal_append_sequence_before_flush() {
    let mut store = store_at(10);
    let returned_lsns = [70, 70, 71];
    let mut append_index = 0;
    let mut flush_called = false;

    let error = store
        .apply_definition_batch_durably(
            &product_batch(10, 11),
            |_kind, _payload| {
                let lsn = returned_lsns[append_index];
                append_index += 1;
                Ok(lsn)
            },
            |_commit_lsn| {
                flush_called = true;
                Ok(71)
            },
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("strictly increasing"));
    assert_eq!(append_index, 3);
    assert!(!flush_called);
    assert_store_unpublished_at(&store, 10);
}

#[test]
fn durable_apply_rejects_zero_wal_append_lsn_before_flush() {
    let mut store = store_at(10);
    let returned_lsns = [0, 71, 72];
    let mut append_index = 0;
    let mut flush_called = false;

    let error = store
        .apply_definition_batch_durably(
            &product_batch(10, 11),
            |_kind, _payload| {
                let lsn = returned_lsns[append_index];
                append_index += 1;
                Ok(lsn)
            },
            |_commit_lsn| {
                flush_called = true;
                Ok(72)
            },
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("LSN must not be zero"));
    assert_eq!(append_index, 3);
    assert!(!flush_called);
    assert_store_unpublished_at(&store, 10);
}

#[test]
fn durable_catalog_wal_payload_rejects_planned_only_publication_boundary() {
    let store = store_at(10);
    let plan = plan_product_batch(&store, 10, 11);
    let mut record = plan.mutation_plan.records().into_iter().next().unwrap();
    match &mut record {
        CatalogMutationRecord::Begin(boundary) => {
            boundary.publication_semantics = CatalogPublicationSemantics::PlannedVersionOnly;
        }
        _ => unreachable!("definition batch WAL sequence must start with Begin"),
    }

    let error = record.encode_durable_payload().unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("durable publication semantics"));
}

#[test]
fn durable_catalog_wal_boundaries_roundtrip_definition_batch_hashes() {
    let store = store_at(10);
    let definition_batch = product_batch(10, 11);
    let expected_source_hash = definition_batch.source_hash();
    let expected_dependency_graph_hash = definition_batch.dependency_graph_hash().unwrap();
    let plan = store.plan_definition_batch(&definition_batch).unwrap();

    let records = plan.mutation_plan.records();
    for record in [records.first().unwrap(), records.last().unwrap()] {
        let decoded = CatalogMutationRecord::decode_durable_payload(
            &record.encode_durable_payload().unwrap(),
        )
        .unwrap();

        match decoded {
            CatalogMutationRecord::Begin(boundary) | CatalogMutationRecord::Commit(boundary) => {
                assert_eq!(boundary.source_hash, expected_source_hash);
                assert_eq!(
                    boundary.dependency_graph_hash,
                    expected_dependency_graph_hash
                );
                assert!(!boundary.source_hash.is_zero());
                assert!(!boundary.dependency_graph_hash.is_zero());
            }
            CatalogMutationRecord::Apply(_) => unreachable!("boundary test selected apply record"),
        }
    }
}

#[test]
fn durable_catalog_wal_boundary_rejects_missing_definition_batch_hash() {
    let store = store_at(10);
    let plan = plan_product_batch(&store, 10, 11);
    let mut record = plan.mutation_plan.records().into_iter().next().unwrap();
    match &mut record {
        CatalogMutationRecord::Begin(boundary) => {
            boundary.source_hash = DefinitionBatchSourceHash::default();
        }
        _ => unreachable!("definition batch WAL sequence must start with Begin"),
    }

    let error = record.encode_durable_payload().unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("source hash"));
}

#[test]
fn recovery_rejects_commit_with_definition_batch_hash_mismatch() {
    let store = store_at(10);
    let plan = plan_product_batch(&store, 10, 11);
    let mut records = plan.mutation_plan.records();
    match records.last_mut().unwrap() {
        CatalogMutationRecord::Commit(boundary) => {
            boundary.source_hash = DefinitionBatchSourceHash::new([0x44; 32]);
        }
        _ => unreachable!("definition batch WAL sequence must end with Commit"),
    }

    let outcome = replay_records_at(10, records);

    assert_eq!(outcome.snapshot.version, version(10));
    assert_has_anomaly(&outcome, CatalogRecoveryAnomalyKind::CommitBoundaryMismatch);
}

#[test]
fn durable_apply_recovery_reconstructs_last_published_catalog_from_storage_wal() {
    let mut store = store_at(10);
    let mut appended = Vec::new();
    let mut next_lsn = 100;

    let report = store
        .apply_definition_batch_durably(
            &product_batch(10, 11),
            |kind, payload| {
                let lsn = next_lsn;
                next_lsn += 1;
                appended.push((kind, payload.to_vec(), lsn));
                Ok(lsn)
            },
            Ok,
        )
        .unwrap();

    let outcome = recover_payloads_at(
        10,
        appended.iter().map(|(kind, payload, _)| {
            CatalogDurableMutationPayload::with_storage_wal_kind_tag(
                payload,
                kind.storage_wal_kind_tag(),
            )
        }),
    );

    assert_eq!(report.receipt.next_version, version(11));
    assert_eq!(outcome.report.anomaly_count(), 0);
    assert_eq!(outcome.snapshot.version, version(11));
    assert!(
        outcome
            .snapshot
            .contains_name(&QualifiedName::parse("Inventory.Product").unwrap())
    );
}

#[test]
fn durable_apply_crash_before_commit_does_not_publish_half_catalog() {
    let mut store = store_at(10);
    let mut appended = Vec::new();
    let mut next_lsn = 30;

    let error = store
        .apply_definition_batch_durably(
            &product_batch(10, 11),
            |kind, payload| {
                let lsn = next_lsn;
                next_lsn += 1;
                appended.push((kind, payload.to_vec(), lsn));
                Ok(lsn)
            },
            |_commit_lsn| {
                Err(AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    "simulated crash before durable catalog commit flush",
                ))
            },
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert_eq!(appended.len(), 3);
    assert_store_unpublished_at(&store, 10);
}

#[test]
fn durable_publication_receipt_can_use_external_marker() {
    let mut store = store_at(10);
    let plan = plan_product_batch(&store, 10, 11);
    let records = plan.mutation_plan.records();
    let marker = CatalogDurabilityMarker::new(44);
    let evidence = CatalogMutationCommitEvidence::from_durable_commit_record(
        records.last().unwrap(),
        records.len(),
        CatalogMutationDurability::ExternalMarker(marker),
    )
    .unwrap();

    let receipt = store
        .publish_durable_mutation_plan(&plan.mutation_plan, evidence)
        .unwrap();

    assert_eq!(receipt.durable_lsn, None);
    assert_eq!(receipt.durable_evidence_marker, Some(marker));
    assert_eq!(
        store.snapshot().publication,
        CatalogSnapshotPublication::Durable(receipt)
    );
}

#[test]
fn durable_publication_rejects_uncommitted_or_non_durable_evidence() {
    let store = store_at(10);
    let plan = plan_product_batch(&store, 10, 11);
    let records = plan.mutation_plan.records();

    let begin_error = CatalogMutationCommitEvidence::from_durable_commit_record(
        records.first().unwrap(),
        records.len(),
        CatalogMutationDurability::StorageWal {
            commit_lsn: 77,
            durable_lsn: 80,
        },
    )
    .unwrap_err();
    assert_eq!(begin_error.kind(), AndromedaErrorKind::Catalog);
    assert!(
        begin_error
            .message()
            .contains("committed mutation evidence")
    );

    let non_durable_evidence = CatalogMutationCommitEvidence::from_durable_commit_record(
        records.last().unwrap(),
        records.len(),
        CatalogMutationDurability::StorageWal {
            commit_lsn: 77,
            durable_lsn: 76,
        },
    )
    .unwrap();
    let mut publish_store = store_at(10);
    let error = publish_store
        .publish_durable_mutation_plan(&plan.mutation_plan, non_durable_evidence)
        .unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("durable LSN"));
    assert_eq!(publish_store.snapshot().version, version(10));
}

#[test]
fn durable_publication_rejects_stale_identity_or_record_count_mismatch() {
    let store = store_at(10);
    let plan = plan_product_batch(&store, 10, 11);
    let records = plan.mutation_plan.records();
    let evidence = CatalogMutationCommitEvidence::from_durable_commit_record(
        records.last().unwrap(),
        records.len(),
        CatalogMutationDurability::StorageWal {
            commit_lsn: 77,
            durable_lsn: 80,
        },
    )
    .unwrap();

    let mut stale_store = store_at(9);
    let stale_error = stale_store
        .publish_durable_mutation_plan(&plan.mutation_plan, evidence)
        .unwrap_err();
    assert_eq!(stale_error.kind(), AndromedaErrorKind::Catalog);
    assert!(stale_error.message().contains("previous version"));

    let mut wrong_database_plan = plan.mutation_plan.clone();
    wrong_database_plan.database_id = DatabaseId::new(DATABASE_ID.get() + 100);
    let mut publish_store = store_at(10);
    let identity_error = publish_store
        .publish_durable_mutation_plan(&wrong_database_plan, evidence)
        .unwrap_err();
    assert_eq!(identity_error.kind(), AndromedaErrorKind::Catalog);
    assert!(identity_error.message().contains("boundary"));

    let wrong_record_count = CatalogMutationCommitEvidence::from_durable_commit_record(
        records.last().unwrap(),
        records.len() - 1,
        CatalogMutationDurability::StorageWal {
            commit_lsn: 77,
            durable_lsn: 80,
        },
    )
    .unwrap();
    let count_error = publish_store
        .publish_durable_mutation_plan(&plan.mutation_plan, wrong_record_count)
        .unwrap_err();
    assert_eq!(count_error.kind(), AndromedaErrorKind::Catalog);
    assert!(count_error.message().contains("record count"));
}

#[test]
fn recovery_replays_committed_durable_catalog_batches_into_snapshot() {
    let mut planner = store_at(10);
    let plan_one = plan_product_batch(&planner, 10, 11);
    planner
        .apply_mutation_plan(&plan_one.mutation_plan)
        .unwrap();
    let plan_two = planner
        .plan_definition_batch(&batch(
            version(11),
            vec![create_table(2, "Inventory.Stock", 12)],
        ))
        .unwrap();

    let encoded_records = plan_one
        .mutation_plan
        .records()
        .into_iter()
        .chain(plan_two.mutation_plan.records())
        .map(|record| {
            (
                record.kind().storage_wal_kind_tag(),
                record.encode_durable_payload().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let inputs = encoded_records.iter().map(|(tag, payload)| {
        CatalogDurableMutationPayload::with_storage_wal_kind_tag(payload, *tag)
    });

    let outcome = recover_payloads_at(10, inputs);

    assert_eq!(outcome.report.anomaly_count(), 0);
    assert_eq!(outcome.report.replayed_batches.len(), 2);
    assert_eq!(outcome.report.final_visible_catalog_version, version(12));
    assert_eq!(outcome.snapshot.version, version(12));
    assert!(
        outcome
            .snapshot
            .contains_name(&QualifiedName::parse("Inventory.Product").unwrap())
    );
    assert!(
        outcome
            .snapshot
            .contains_name(&QualifiedName::parse("Inventory.Stock").unwrap())
    );
}

#[test]
fn recovery_ignores_incomplete_catalog_batches_without_commit() {
    let planner = store_at(10);
    let plan = plan_product_batch(&planner, 10, 11);
    let records = plan
        .mutation_plan
        .records()
        .into_iter()
        .take(plan.mutation_plan.record_count() - 1)
        .collect::<Vec<_>>();

    let outcome = replay_records_at(10, records);

    assert!(outcome.report.replayed_batches.is_empty());
    assert_eq!(outcome.report.skipped_incomplete_batches.len(), 1);
    assert_eq!(
        outcome.report.skipped_incomplete_batches[0].reason,
        CatalogSkippedBatchReason::EndOfLogBeforeCommit
    );
    assert_empty_snapshot_at(&outcome, 10);
}

#[test]
fn recovery_rejects_committed_batch_missing_tail_apply_record() {
    let planner = store_at(10);
    let plan = planner
        .plan_definition_batch(&batch(
            version(10),
            vec![
                create_table(1, "Inventory.Product", 11),
                create_table(2, "Inventory.Stock", 11),
                create_table(3, "Inventory.Audit", 11),
            ],
        ))
        .unwrap();
    let mut records = plan.mutation_plan.records();
    records.remove(3);

    let outcome = replay_records_at(10, records);

    assert!(outcome.report.replayed_batches.is_empty());
    assert_has_anomaly(
        &outcome,
        CatalogRecoveryAnomalyKind::ApplyRecordCountMismatch,
    );
    assert_eq!(
        outcome.report.skipped_incomplete_batches[0].reason,
        CatalogSkippedBatchReason::ApplyRecordCountMismatch
    );
    assert_empty_snapshot_at(&outcome, 10);
}

#[test]
fn recovery_rejects_apply_count_above_replay_limit_before_accumulating_batch() {
    let planner = store_at(10);
    let plan = plan_product_batch(&planner, 10, 11);
    let mut records = plan.mutation_plan.records();
    match &mut records[0] {
        CatalogMutationRecord::Begin(boundary) => {
            boundary.expected_apply_count = CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH + 1;
        }
        _ => unreachable!("definition batch WAL sequence must start with Begin"),
    }

    let outcome = replay_records_at(10, records);

    assert!(outcome.report.replayed_batches.is_empty());
    assert_has_anomaly(
        &outcome,
        CatalogRecoveryAnomalyKind::ApplyRecordLimitExceeded,
    );
    assert_eq!(
        outcome.report.skipped_anomalous_batches[0].reason,
        CatalogSkippedBatchReason::ApplyRecordLimitExceeded
    );
    assert_empty_snapshot_at(&outcome, 10);
}

#[test]
fn recovery_reports_commit_and_apply_without_begin() {
    let planner = store_at(10);
    let records = plan_product_batch(&planner, 10, 11).mutation_plan.records();

    let outcome = replay_records_at(10, vec![records[1].clone(), records[2].clone()]);

    assert_eq!(outcome.report.anomaly_count(), 2);
    assert_has_anomaly(&outcome, CatalogRecoveryAnomalyKind::ApplyWithoutBegin);
    assert_has_anomaly(&outcome, CatalogRecoveryAnomalyKind::CommitWithoutBegin);
    assert_eq!(outcome.snapshot.version, version(10));
}

#[test]
fn recovery_rejects_physically_reordered_apply_records() {
    let planner = store_at(10);
    let plan = planner
        .plan_definition_batch(&batch(
            version(10),
            vec![
                create_table(1, "Inventory.Product", 11),
                create_table(2, "Inventory.Stock", 11),
            ],
        ))
        .unwrap();
    let mut records = plan.mutation_plan.records();
    records.swap(1, 2);

    let outcome = replay_records_at(10, records);

    assert!(outcome.report.replayed_batches.is_empty());
    assert_has_anomaly(
        &outcome,
        CatalogRecoveryAnomalyKind::ApplyRecordOrderMismatch,
    );
    assert_eq!(
        outcome.report.skipped_anomalous_batches[0].reason,
        CatalogSkippedBatchReason::ApplyRecordOrderMismatch
    );
    assert_empty_snapshot_at(&outcome, 10);
}

#[test]
fn recovery_rejects_begin_commit_boundary_mismatch_all_or_nothing() {
    let planner = store_at(10);
    let mut records = plan_product_batch(&planner, 10, 11).mutation_plan.records();
    match records.last_mut().unwrap() {
        CatalogMutationRecord::Commit(boundary) => {
            boundary.next_version = CatalogVersion::new(boundary.next_version.get() + 1);
        }
        _ => unreachable!("definition batch WAL sequence must end with Commit"),
    }

    let outcome = replay_records_at(10, records);

    assert!(outcome.report.replayed_batches.is_empty());
    assert_has_anomaly(&outcome, CatalogRecoveryAnomalyKind::CommitBoundaryMismatch);
    assert_eq!(
        outcome.report.skipped_anomalous_batches[0].reason,
        CatalogSkippedBatchReason::CommitBoundaryMismatch
    );
    assert_empty_snapshot_at(&outcome, 10);
}

#[test]
fn recovery_rejects_tampered_apply_with_stale_definition_batch_hashes() {
    let planner = store_at(10);
    let plan = plan_product_batch(&planner, 10, 11);
    let mut records = plan.mutation_plan.records();
    let CatalogMutationRecord::Apply(delta) = &mut records[1] else {
        panic!("definition batch WAL sequence must contain an Apply record");
    };
    let CatalogMutationOperation::CreateObject { definition, .. } = &mut delta.operation else {
        panic!("test mutation must create an object");
    };
    let CatalogDefinition::Table(table) = definition else {
        panic!("test mutation must create a table");
    };
    table.columns.push(column("WarehouseId", 1));

    let outcome = replay_records_at(10, records);

    assert!(outcome.report.replayed_batches.is_empty());
    assert_has_anomaly(
        &outcome,
        CatalogRecoveryAnomalyKind::DefinitionBatchHashMismatch,
    );
    assert_eq!(
        outcome.report.skipped_anomalous_batches[0].reason,
        CatalogSkippedBatchReason::DefinitionBatchHashMismatch
    );
    assert_empty_snapshot_at(&outcome, 10);
}

#[test]
fn recovery_reports_duplicate_and_sparse_apply_indexes() {
    let planner = store_at(10);
    let plan = planner
        .plan_definition_batch(&batch(
            version(10),
            vec![
                create_table(1, "Inventory.Product", 11),
                create_table(2, "Inventory.Stock", 11),
            ],
        ))
        .unwrap();

    let mut duplicate_records = plan.mutation_plan.records();
    if let CatalogMutationRecord::Apply(delta) = &mut duplicate_records[2] {
        delta.operation_index = 0;
    }
    let duplicate_outcome = replay_records_at(10, duplicate_records);
    assert_has_anomaly(
        &duplicate_outcome,
        CatalogRecoveryAnomalyKind::DuplicateApplyIndex,
    );
    assert_eq!(
        duplicate_outcome.report.skipped_anomalous_batches[0].reason,
        CatalogSkippedBatchReason::DuplicateApplyIndex
    );

    let mut sparse_records = plan.mutation_plan.records();
    if let CatalogMutationRecord::Apply(delta) = &mut sparse_records[2] {
        delta.operation_index = 2;
    }
    let sparse_outcome = replay_records_at(10, sparse_records);
    assert_has_anomaly(
        &sparse_outcome,
        CatalogRecoveryAnomalyKind::SparseApplyIndexes,
    );
    assert_eq!(
        sparse_outcome.report.skipped_incomplete_batches[0].reason,
        CatalogSkippedBatchReason::SparseApplyIndexes
    );
}

#[test]
fn recovery_reports_wrong_identity_version_gap_outer_kind_and_payload_corruption() {
    let planner = store_at(10);
    let plan = plan_product_batch(&planner, 10, 11);

    let mut wrong_identity_records = plan.mutation_plan.records();
    for record in &mut wrong_identity_records {
        match record {
            CatalogMutationRecord::Begin(boundary) | CatalogMutationRecord::Commit(boundary) => {
                boundary.database_id = DatabaseId::new(DATABASE_ID.get() + 100);
            }
            CatalogMutationRecord::Apply(_) => {}
        }
    }
    let wrong_identity_outcome = replay_records_at(10, wrong_identity_records);
    assert_has_anomaly(
        &wrong_identity_outcome,
        CatalogRecoveryAnomalyKind::WrongCatalogIdentity,
    );

    let version_gap_outcome = replay_records_at(9, plan.mutation_plan.records());
    assert_has_anomaly(&version_gap_outcome, CatalogRecoveryAnomalyKind::VersionGap);

    let begin = plan.mutation_plan.records().into_iter().next().unwrap();
    let encoded_begin = begin.encode_durable_payload().unwrap();
    let wrong_outer_kind = recover_payloads_at(
        10,
        vec![CatalogDurableMutationPayload::with_storage_wal_kind_tag(
            &encoded_begin,
            CatalogMutationRecordKind::CatalogChangeApply.storage_wal_kind_tag(),
        )],
    );
    assert_has_anomaly(
        &wrong_outer_kind,
        CatalogRecoveryAnomalyKind::OuterStorageKindMismatch,
    );

    const VERSION_OFFSET: usize = 8;
    const CHECKSUM_OFFSET: usize = 20;

    let mut magic_mismatch = encoded_begin.clone();
    magic_mismatch[0] ^= 0xFF;

    let mut unsupported_version = encoded_begin.clone();
    unsupported_version[VERSION_OFFSET..VERSION_OFFSET + 2].copy_from_slice(&99_u16.to_le_bytes());

    let mut checksum_mismatch = encoded_begin.clone();
    checksum_mismatch[CHECKSUM_OFFSET] ^= 0xFF;

    for (payload, expected_kind) in [
        (
            magic_mismatch,
            CatalogRecoveryAnomalyKind::PayloadMagicMismatch,
        ),
        (
            unsupported_version,
            CatalogRecoveryAnomalyKind::PayloadFormatVersionMismatch,
        ),
        (
            checksum_mismatch,
            CatalogRecoveryAnomalyKind::PayloadChecksumMismatch,
        ),
    ] {
        let corrupt_outcome =
            recover_payloads_at(10, vec![CatalogDurableMutationPayload::new(&payload)]);
        assert_has_anomaly(&corrupt_outcome, expected_kind);
        assert_eq!(corrupt_outcome.snapshot.version, version(10));
    }
}

#[test]
fn duplicate_object_ids_and_names_are_rejected() {
    let store = store_at(10);

    let duplicate_id_error = store
        .plan_definition_batch(&batch(
            version(10),
            vec![
                create_table(1, "Inventory.Product", 11),
                create_table(1, "Inventory.Stock", 11),
            ],
        ))
        .unwrap_err();
    assert_eq!(duplicate_id_error.kind(), AndromedaErrorKind::Catalog);
    assert!(duplicate_id_error.message().contains("object id"));

    let duplicate_name_error = store
        .plan_definition_batch(&batch(
            version(10),
            vec![
                create_table(1, "Inventory.Product", 11),
                create_table(2, "Inventory.Product", 11),
            ],
        ))
        .unwrap_err();
    assert_eq!(duplicate_name_error.kind(), AndromedaErrorKind::Catalog);
    assert!(duplicate_name_error.message().contains("object name"));
}

#[test]
fn definition_batch_source_hash_preserves_ordered_source_identity() {
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
            DefinitionOperation::Create(CatalogDefinition::Procedure(procedure(
                3,
                "Inventory.ReserveStock",
                catalog_version,
                vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
            ))),
        ],
    );
    let audit_dependency = batch(
        base_version,
        vec![
            DefinitionOperation::Create(stock_request),
            DefinitionOperation::Create(audit_request),
            DefinitionOperation::Create(CatalogDefinition::Procedure(procedure(
                3,
                "Inventory.ReserveStock",
                catalog_version,
                vec![QualifiedName::parse("Inventory.AuditRequest").unwrap()],
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
fn durable_apply_report_carries_definition_batch_integrity_hashes() {
    let mut store = store_at(10);
    let definition_batch = product_batch(10, 11);
    let expected_source_hash = definition_batch.source_hash();
    let expected_dependency_graph_hash = definition_batch.dependency_graph_hash().unwrap();
    let mut next_lsn = 70;

    let report = store
        .apply_definition_batch_durably(
            &definition_batch,
            |_kind, _payload| {
                let lsn = next_lsn;
                next_lsn += 1;
                Ok(lsn)
            },
            Ok,
        )
        .unwrap();

    assert_eq!(report.source_hash, expected_source_hash);
    assert_eq!(report.dependency_graph_hash, expected_dependency_graph_hash);
    assert_eq!(report.receipt.source_hash, expected_source_hash);
    assert_eq!(
        report.receipt.dependency_graph_hash,
        expected_dependency_graph_hash
    );
    assert!(!report.source_hash.is_zero());
    assert!(!report.dependency_graph_hash.is_zero());
    assert_eq!(report.receipt.next_version, version(11));
    assert_eq!(store.snapshot().version, version(11));
}

#[test]
fn snapshot_state_rejects_catalog_identity_mismatch() {
    let store = store_at(10);
    let operation = create_inventory_product(11);

    let mut wrong_database = batch(version(10), vec![operation.clone()]);
    wrong_database.database_id = DatabaseId::new(DATABASE_ID.get() + 100);
    let error = store.plan_definition_batch(&wrong_database).unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("database id"));

    let mut wrong_namespace = batch(version(10), vec![operation]);
    wrong_namespace.namespace_id = NamespaceId::new(NAMESPACE_ID.get() + 100);
    let error = store.plan_definition_batch(&wrong_namespace).unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("namespace id"));
}

#[test]
fn snapshot_state_rejects_create_collisions_with_existing_objects() {
    let mut store = store_at(10);
    store
        .apply_definition_batch(&product_batch(10, 11))
        .unwrap();

    let duplicate_id_error = store
        .plan_definition_batch(&batch(
            version(11),
            vec![create_table(1, "Inventory.Stock", 12)],
        ))
        .unwrap_err();
    assert_eq!(duplicate_id_error.kind(), AndromedaErrorKind::Catalog);
    assert!(duplicate_id_error.message().contains("already present"));
    assert!(duplicate_id_error.message().contains("object id"));

    let duplicate_name_error = store
        .plan_definition_batch(&batch(
            version(11),
            vec![create_table(2, "Inventory.Product", 12)],
        ))
        .unwrap_err();
    assert_eq!(duplicate_name_error.kind(), AndromedaErrorKind::Catalog);
    assert!(duplicate_name_error.message().contains("already present"));
    assert!(duplicate_name_error.message().contains("object name"));
}

#[test]
fn snapshot_state_rejects_deprecation_of_missing_target() {
    let store = store_at(10);

    let error = store
        .plan_definition_batch(&batch(
            version(10),
            vec![DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                object: object(99, "Inventory.Missing", ObjectKind::Table, version(10)),
            })],
        ))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("unknown object id"));
}

#[test]
fn dependency_ordering_is_rejected_for_late_structured_inputs() {
    let store = store_at(10);

    let error = store
        .plan_definition_batch(&batch(
            version(10),
            vec![
                DefinitionOperation::Create(CatalogDefinition::Procedure(procedure(
                    2,
                    "Inventory.ReserveStock",
                    version(11),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ))),
                DefinitionOperation::Create(CatalogDefinition::StructuredObject(
                    structured_object(1, "Inventory.StockRequest", version(11)),
                )),
            ],
        ))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("dependencies before dependent"));
}

#[test]
fn snapshot_state_validates_external_structured_input_dependencies() {
    let missing_store = store_at(10);
    let error = missing_store
        .plan_definition_batch(&batch(
            version(10),
            vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
                procedure(
                    2,
                    "Inventory.ReserveStock",
                    version(11),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ),
            ))],
        ))
        .unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("dependency"));
    assert!(error.message().contains("missing"));

    let mut store = store_at(10);
    store
        .apply_definition_batch(&batch(
            version(10),
            vec![DefinitionOperation::Create(
                CatalogDefinition::StructuredObject(structured_object(
                    1,
                    "Inventory.StockRequest",
                    version(11),
                )),
            )],
        ))
        .unwrap();

    let plan = store
        .plan_definition_batch(&batch(
            version(11),
            vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
                procedure(
                    2,
                    "Inventory.ReserveStock",
                    version(12),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ),
            ))],
        ))
        .unwrap();

    assert_eq!(plan.previous_version, version(11));
    assert_eq!(plan.next_version, version(12));
    assert_eq!(plan.created_objects.len(), 1);
}

#[test]
fn procedure_structured_inputs_are_catalog_dependencies() {
    let contract = procedure(
        2,
        "Inventory.ReserveStock",
        version(11),
        vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
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
fn snapshot_state_rejects_deprecation_with_active_dependents() {
    let mut store = store_at(10);
    store
        .apply_definition_batch(&batch(
            version(10),
            vec![DefinitionOperation::Create(
                CatalogDefinition::StructuredObject(structured_object(
                    1,
                    "Inventory.StockRequest",
                    version(11),
                )),
            )],
        ))
        .unwrap();
    store
        .apply_definition_batch(&batch(
            version(11),
            vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
                procedure(
                    2,
                    "Inventory.ReserveStock",
                    version(12),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ),
            ))],
        ))
        .unwrap();

    let error = store
        .plan_definition_batch(&batch(
            version(12),
            vec![DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                object: object(
                    1,
                    "Inventory.StockRequest",
                    ObjectKind::StructuredObject,
                    version(11),
                ),
            })],
        ))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("active dependents"));
}

#[test]
fn snapshot_state_allows_joint_deprecation_of_dependency_and_dependent() {
    let mut store = store_at(10);
    store
        .apply_definition_batch(&batch(
            version(10),
            vec![DefinitionOperation::Create(
                CatalogDefinition::StructuredObject(structured_object(
                    1,
                    "Inventory.StockRequest",
                    version(11),
                )),
            )],
        ))
        .unwrap();
    store
        .apply_definition_batch(&batch(
            version(11),
            vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
                procedure(
                    2,
                    "Inventory.ReserveStock",
                    version(12),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ),
            ))],
        ))
        .unwrap();

    let plan = store
        .plan_definition_batch(&batch(
            version(12),
            vec![
                DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                    object: object(
                        2,
                        "Inventory.ReserveStock",
                        ObjectKind::Procedure,
                        version(12),
                    ),
                }),
                DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                    object: object(
                        1,
                        "Inventory.StockRequest",
                        ObjectKind::StructuredObject,
                        version(11),
                    ),
                }),
            ],
        ))
        .unwrap();

    assert_eq!(plan.deprecated_objects.len(), 2);
    assert_eq!(plan.next_version, version(13));
}

#[test]
fn snapshot_state_rejects_new_dependent_when_dependency_is_deprecated() {
    let mut store = store_at(10);
    store
        .apply_definition_batch(&batch(
            version(10),
            vec![DefinitionOperation::Create(
                CatalogDefinition::StructuredObject(structured_object(
                    1,
                    "Inventory.StockRequest",
                    version(11),
                )),
            )],
        ))
        .unwrap();

    let error = store
        .plan_definition_batch(&batch(
            version(11),
            vec![
                DefinitionOperation::Create(CatalogDefinition::Procedure(procedure(
                    2,
                    "Inventory.ReserveStock",
                    version(12),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ))),
                DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                    object: object(
                        1,
                        "Inventory.StockRequest",
                        ObjectKind::StructuredObject,
                        version(11),
                    ),
                }),
            ],
        ))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("active dependents"));
}

#[test]
fn catalog_api_has_no_storage_or_exec_coupling() {
    let manifest = include_str!("../Cargo.toml");
    let runtime_dependencies = manifest
        .split_once("[dev-dependencies]")
        .map_or(manifest, |(runtime_dependencies, _)| runtime_dependencies);

    assert!(!runtime_dependencies.contains("andromeda-storage"));
    assert!(!runtime_dependencies.contains("andromeda-exec"));

    let store = store_at(1);
    assert_eq!(
        store.snapshot().publication,
        CatalogSnapshotPublication::InMemoryOnly
    );
}
