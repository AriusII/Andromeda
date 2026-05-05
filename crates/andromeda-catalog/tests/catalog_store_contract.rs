use andromeda_catalog::{
    AccessMode, CatalogDefinition, CatalogDependencyKind, CatalogDurabilityMarker,
    CatalogDurableMutationPayload, CatalogLifecycleTarget, CatalogMutationCommitEvidence,
    CatalogMutationDurability, CatalogMutationRecord, CatalogMutationRecordKind, CatalogObjectRef,
    CatalogPublicationSemantics, CatalogRecoveryAnomalyKind, CatalogSkippedBatchReason,
    CatalogSnapshotPublication, CatalogSystemStore, CompatibilityPolicy, DefinitionBatch,
    DefinitionBatchId, DefinitionOperation, IsolationPolicy, MultiResultPolicy, ObjectKind,
    ProcedureContract, ProcedureContractCandidate, ProcedureErrorPolicy, ProtocolLayoutRef,
    QualifiedName, ResultMetadataPolicy, StatsVersion, StructuredObjectDefinition, TableDefinition,
    TransactionPolicy, recover_catalog_snapshot_from_durable_payloads,
    replay_catalog_mutation_records,
};
use andromeda_core::{
    AndromedaErrorKind, CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash,
    DatabaseId, NamespaceId, ProcedureId, ScalarType, TransactionId, TypeDescriptor,
};
use andromeda_storage::{
    Lsn, WalRecord, WalRecordKind, decode_wal_record_frame, encode_wal_record,
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
fn catalog_mutation_records_fit_storage_wal_catalog_kinds() {
    let store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    let plan = store
        .plan_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Create(CatalogDefinition::Table(
                table(1, "Inventory.Product", CatalogVersion::new(11)),
            ))],
        ))
        .unwrap();

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
fn durable_publication_advances_visible_snapshot_with_receipt() {
    let mut store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    let plan = store
        .plan_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Create(CatalogDefinition::Table(
                table(1, "Inventory.Product", CatalogVersion::new(11)),
            ))],
        ))
        .unwrap();
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

    assert_eq!(store.snapshot().version, CatalogVersion::new(11));
    assert_eq!(receipt.previous_version, CatalogVersion::new(10));
    assert_eq!(receipt.next_version, CatalogVersion::new(11));
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
fn durable_publication_receipt_can_use_external_marker() {
    let mut store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    let plan = store
        .plan_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Create(CatalogDefinition::Table(
                table(1, "Inventory.Product", CatalogVersion::new(11)),
            ))],
        ))
        .unwrap();
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
    let store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    let plan = store
        .plan_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Create(CatalogDefinition::Table(
                table(1, "Inventory.Product", CatalogVersion::new(11)),
            ))],
        ))
        .unwrap();
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
    let mut publish_store =
        CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    let error = publish_store
        .publish_durable_mutation_plan(&plan.mutation_plan, non_durable_evidence)
        .unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("durable LSN"));
    assert_eq!(publish_store.snapshot().version, CatalogVersion::new(10));
}

#[test]
fn durable_publication_rejects_stale_identity_or_record_count_mismatch() {
    let store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    let plan = store
        .plan_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Create(CatalogDefinition::Table(
                table(1, "Inventory.Product", CatalogVersion::new(11)),
            ))],
        ))
        .unwrap();
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

    let mut stale_store =
        CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(9));
    let stale_error = stale_store
        .publish_durable_mutation_plan(&plan.mutation_plan, evidence)
        .unwrap_err();
    assert_eq!(stale_error.kind(), AndromedaErrorKind::Catalog);
    assert!(stale_error.message().contains("previous version"));

    let mut wrong_database_plan = plan.mutation_plan.clone();
    wrong_database_plan.database_id = DatabaseId::new(DATABASE_ID.get() + 100);
    let mut publish_store =
        CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
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
    let mut planner = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    let plan_one = planner
        .plan_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Create(CatalogDefinition::Table(
                table(1, "Inventory.Product", CatalogVersion::new(11)),
            ))],
        ))
        .unwrap();
    planner
        .apply_mutation_plan(&plan_one.mutation_plan)
        .unwrap();
    let plan_two = planner
        .plan_definition_batch(&batch(
            CatalogVersion::new(11),
            vec![DefinitionOperation::Create(CatalogDefinition::Table(
                table(2, "Inventory.Stock", CatalogVersion::new(12)),
            ))],
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

    let outcome = recover_catalog_snapshot_from_durable_payloads(
        CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10))
            .into_snapshot(),
        inputs,
    );

    assert_eq!(outcome.report.anomaly_count(), 0);
    assert_eq!(outcome.report.replayed_batches.len(), 2);
    assert_eq!(
        outcome.report.final_visible_catalog_version,
        CatalogVersion::new(12)
    );
    assert_eq!(outcome.snapshot.version, CatalogVersion::new(12));
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
    let planner = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    let plan = planner
        .plan_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Create(CatalogDefinition::Table(
                table(1, "Inventory.Product", CatalogVersion::new(11)),
            ))],
        ))
        .unwrap();
    let records = plan
        .mutation_plan
        .records()
        .into_iter()
        .take(plan.mutation_plan.record_count() - 1)
        .collect::<Vec<_>>();

    let outcome = replay_catalog_mutation_records(
        CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10))
            .into_snapshot(),
        records,
    );

    assert!(outcome.report.replayed_batches.is_empty());
    assert_eq!(outcome.report.skipped_incomplete_batches.len(), 1);
    assert_eq!(
        outcome.report.skipped_incomplete_batches[0].reason,
        CatalogSkippedBatchReason::EndOfLogBeforeCommit
    );
    assert_eq!(outcome.snapshot.version, CatalogVersion::new(10));
    assert_eq!(outcome.snapshot.object_count(), 0);
}

#[test]
fn recovery_reports_commit_and_apply_without_begin() {
    let planner = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    let records = planner
        .plan_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Create(CatalogDefinition::Table(
                table(1, "Inventory.Product", CatalogVersion::new(11)),
            ))],
        ))
        .unwrap()
        .mutation_plan
        .records();

    let outcome = replay_catalog_mutation_records(
        CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10))
            .into_snapshot(),
        vec![records[1].clone(), records[2].clone()],
    );

    assert_eq!(outcome.report.anomaly_count(), 2);
    assert!(
        outcome
            .report
            .anomalies
            .iter()
            .any(|anomaly| anomaly.kind == CatalogRecoveryAnomalyKind::ApplyWithoutBegin)
    );
    assert!(
        outcome
            .report
            .anomalies
            .iter()
            .any(|anomaly| anomaly.kind == CatalogRecoveryAnomalyKind::CommitWithoutBegin)
    );
    assert_eq!(outcome.snapshot.version, CatalogVersion::new(10));
}

#[test]
fn recovery_reports_duplicate_and_sparse_apply_indexes() {
    let planner = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    let plan = planner
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

    let mut duplicate_records = plan.mutation_plan.records();
    if let CatalogMutationRecord::Apply(delta) = &mut duplicate_records[2] {
        delta.operation_index = 0;
    }
    let duplicate_outcome = replay_catalog_mutation_records(
        CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10))
            .into_snapshot(),
        duplicate_records,
    );
    assert!(
        duplicate_outcome
            .report
            .anomalies
            .iter()
            .any(|anomaly| anomaly.kind == CatalogRecoveryAnomalyKind::DuplicateApplyIndex)
    );
    assert_eq!(
        duplicate_outcome.report.skipped_anomalous_batches[0].reason,
        CatalogSkippedBatchReason::DuplicateApplyIndex
    );

    let mut sparse_records = plan.mutation_plan.records();
    if let CatalogMutationRecord::Apply(delta) = &mut sparse_records[2] {
        delta.operation_index = 2;
    }
    let sparse_outcome = replay_catalog_mutation_records(
        CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10))
            .into_snapshot(),
        sparse_records,
    );
    assert!(
        sparse_outcome
            .report
            .anomalies
            .iter()
            .any(|anomaly| anomaly.kind == CatalogRecoveryAnomalyKind::SparseApplyIndexes)
    );
    assert_eq!(
        sparse_outcome.report.skipped_incomplete_batches[0].reason,
        CatalogSkippedBatchReason::SparseApplyIndexes
    );
}

#[test]
fn recovery_reports_wrong_identity_version_gap_outer_kind_and_payload_corruption() {
    let planner = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    let plan = planner
        .plan_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Create(CatalogDefinition::Table(
                table(1, "Inventory.Product", CatalogVersion::new(11)),
            ))],
        ))
        .unwrap();

    let mut wrong_identity_records = plan.mutation_plan.records();
    for record in &mut wrong_identity_records {
        match record {
            CatalogMutationRecord::Begin(boundary) | CatalogMutationRecord::Commit(boundary) => {
                boundary.database_id = DatabaseId::new(DATABASE_ID.get() + 100);
            }
            CatalogMutationRecord::Apply(_) => {}
        }
    }
    let wrong_identity_outcome = replay_catalog_mutation_records(
        CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10))
            .into_snapshot(),
        wrong_identity_records,
    );
    assert!(
        wrong_identity_outcome
            .report
            .anomalies
            .iter()
            .any(|anomaly| anomaly.kind == CatalogRecoveryAnomalyKind::WrongCatalogIdentity)
    );

    let version_gap_outcome = replay_catalog_mutation_records(
        CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(9))
            .into_snapshot(),
        plan.mutation_plan.records(),
    );
    assert!(
        version_gap_outcome
            .report
            .anomalies
            .iter()
            .any(|anomaly| anomaly.kind == CatalogRecoveryAnomalyKind::VersionGap)
    );

    let begin = plan.mutation_plan.records().into_iter().next().unwrap();
    let encoded_begin = begin.encode_durable_payload().unwrap();
    let wrong_outer_kind = recover_catalog_snapshot_from_durable_payloads(
        CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10))
            .into_snapshot(),
        vec![CatalogDurableMutationPayload::with_storage_wal_kind_tag(
            &encoded_begin,
            CatalogMutationRecordKind::CatalogChangeApply.storage_wal_kind_tag(),
        )],
    );
    assert!(
        wrong_outer_kind
            .report
            .anomalies
            .iter()
            .any(|anomaly| anomaly.kind == CatalogRecoveryAnomalyKind::OuterStorageKindMismatch)
    );

    let mut corrupted_payload = encoded_begin.clone();
    let last = corrupted_payload.len() - 1;
    corrupted_payload[last] ^= 0xFF;
    let corrupt_outcome = recover_catalog_snapshot_from_durable_payloads(
        CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10))
            .into_snapshot(),
        vec![CatalogDurableMutationPayload::new(&corrupted_payload)],
    );
    assert!(
        corrupt_outcome
            .report
            .anomalies
            .iter()
            .any(|anomaly| anomaly.kind == CatalogRecoveryAnomalyKind::PayloadCorruption)
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
fn snapshot_state_rejects_catalog_identity_mismatch() {
    let store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    let operation = DefinitionOperation::Create(CatalogDefinition::Table(table(
        1,
        "Inventory.Product",
        CatalogVersion::new(11),
    )));

    let mut wrong_database = batch(CatalogVersion::new(10), vec![operation.clone()]);
    wrong_database.database_id = DatabaseId::new(DATABASE_ID.get() + 100);
    let error = store.plan_definition_batch(&wrong_database).unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("database id"));

    let mut wrong_namespace = batch(CatalogVersion::new(10), vec![operation]);
    wrong_namespace.namespace_id = NamespaceId::new(NAMESPACE_ID.get() + 100);
    let error = store.plan_definition_batch(&wrong_namespace).unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("namespace id"));
}

#[test]
fn snapshot_state_rejects_create_collisions_with_existing_objects() {
    let mut store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    store
        .apply_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Create(CatalogDefinition::Table(
                table(1, "Inventory.Product", CatalogVersion::new(11)),
            ))],
        ))
        .unwrap();

    let duplicate_id_error = store
        .plan_definition_batch(&batch(
            CatalogVersion::new(11),
            vec![DefinitionOperation::Create(CatalogDefinition::Table(
                table(1, "Inventory.Stock", CatalogVersion::new(12)),
            ))],
        ))
        .unwrap_err();
    assert_eq!(duplicate_id_error.kind(), AndromedaErrorKind::Catalog);
    assert!(duplicate_id_error.message().contains("already present"));
    assert!(duplicate_id_error.message().contains("object id"));

    let duplicate_name_error = store
        .plan_definition_batch(&batch(
            CatalogVersion::new(11),
            vec![DefinitionOperation::Create(CatalogDefinition::Table(
                table(2, "Inventory.Product", CatalogVersion::new(12)),
            ))],
        ))
        .unwrap_err();
    assert_eq!(duplicate_name_error.kind(), AndromedaErrorKind::Catalog);
    assert!(duplicate_name_error.message().contains("already present"));
    assert!(duplicate_name_error.message().contains("object name"));
}

#[test]
fn snapshot_state_rejects_deprecation_of_missing_target() {
    let store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));

    let error = store
        .plan_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                object: object(
                    99,
                    "Inventory.Missing",
                    ObjectKind::Table,
                    CatalogVersion::new(10),
                ),
            })],
        ))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("unknown object id"));
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
fn snapshot_state_validates_external_structured_input_dependencies() {
    let missing_store =
        CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    let error = missing_store
        .plan_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
                procedure(
                    2,
                    "Inventory.ReserveStock",
                    CatalogVersion::new(11),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ),
            ))],
        ))
        .unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("dependency"));
    assert!(error.message().contains("missing"));

    let mut store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    store
        .apply_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Create(
                CatalogDefinition::StructuredObject(structured_object(
                    1,
                    "Inventory.StockRequest",
                    CatalogVersion::new(11),
                )),
            )],
        ))
        .unwrap();

    let plan = store
        .plan_definition_batch(&batch(
            CatalogVersion::new(11),
            vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
                procedure(
                    2,
                    "Inventory.ReserveStock",
                    CatalogVersion::new(12),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ),
            ))],
        ))
        .unwrap();

    assert_eq!(plan.previous_version, CatalogVersion::new(11));
    assert_eq!(plan.next_version, CatalogVersion::new(12));
    assert_eq!(plan.created_objects.len(), 1);
}

#[test]
fn procedure_structured_inputs_are_catalog_dependencies() {
    let contract = procedure(
        2,
        "Inventory.ReserveStock",
        CatalogVersion::new(11),
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
    let mut store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    store
        .apply_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Create(
                CatalogDefinition::StructuredObject(structured_object(
                    1,
                    "Inventory.StockRequest",
                    CatalogVersion::new(11),
                )),
            )],
        ))
        .unwrap();
    store
        .apply_definition_batch(&batch(
            CatalogVersion::new(11),
            vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
                procedure(
                    2,
                    "Inventory.ReserveStock",
                    CatalogVersion::new(12),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ),
            ))],
        ))
        .unwrap();

    let error = store
        .plan_definition_batch(&batch(
            CatalogVersion::new(12),
            vec![DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                object: object(
                    1,
                    "Inventory.StockRequest",
                    ObjectKind::StructuredObject,
                    CatalogVersion::new(11),
                ),
            })],
        ))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("active dependents"));
}

#[test]
fn snapshot_state_allows_joint_deprecation_of_dependency_and_dependent() {
    let mut store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    store
        .apply_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Create(
                CatalogDefinition::StructuredObject(structured_object(
                    1,
                    "Inventory.StockRequest",
                    CatalogVersion::new(11),
                )),
            )],
        ))
        .unwrap();
    store
        .apply_definition_batch(&batch(
            CatalogVersion::new(11),
            vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
                procedure(
                    2,
                    "Inventory.ReserveStock",
                    CatalogVersion::new(12),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ),
            ))],
        ))
        .unwrap();

    let plan = store
        .plan_definition_batch(&batch(
            CatalogVersion::new(12),
            vec![
                DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                    object: object(
                        2,
                        "Inventory.ReserveStock",
                        ObjectKind::Procedure,
                        CatalogVersion::new(12),
                    ),
                }),
                DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                    object: object(
                        1,
                        "Inventory.StockRequest",
                        ObjectKind::StructuredObject,
                        CatalogVersion::new(11),
                    ),
                }),
            ],
        ))
        .unwrap();

    assert_eq!(plan.deprecated_objects.len(), 2);
    assert_eq!(plan.next_version, CatalogVersion::new(13));
}

#[test]
fn snapshot_state_rejects_new_dependent_when_dependency_is_deprecated() {
    let mut store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(10));
    store
        .apply_definition_batch(&batch(
            CatalogVersion::new(10),
            vec![DefinitionOperation::Create(
                CatalogDefinition::StructuredObject(structured_object(
                    1,
                    "Inventory.StockRequest",
                    CatalogVersion::new(11),
                )),
            )],
        ))
        .unwrap();

    let error = store
        .plan_definition_batch(&batch(
            CatalogVersion::new(11),
            vec![
                DefinitionOperation::Create(CatalogDefinition::Procedure(procedure(
                    2,
                    "Inventory.ReserveStock",
                    CatalogVersion::new(12),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ))),
                DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                    object: object(
                        1,
                        "Inventory.StockRequest",
                        ObjectKind::StructuredObject,
                        CatalogVersion::new(11),
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

    let store = CatalogSystemStore::empty(DATABASE_ID, NAMESPACE_ID, CatalogVersion::new(1));
    assert_eq!(
        store.snapshot().publication,
        CatalogSnapshotPublication::InMemoryOnly
    );
}
