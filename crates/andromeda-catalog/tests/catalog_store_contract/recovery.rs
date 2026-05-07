use super::common::*;

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
fn durable_apply_recovery_reconstructs_last_published_catalog_from_storage_wal_frame_roundtrip() {
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

    let roundtripped = roundtrip_appended_payloads_through_storage_wal(appended);
    let outcome = recover_payloads_at(
        10,
        roundtripped.iter().map(|(payload, storage_wal_kind_tag)| {
            CatalogDurableMutationPayload::with_storage_wal_kind_tag(payload, *storage_wal_kind_tag)
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

    let pre_commit_record_count = appended.len() - 1;
    let roundtripped = roundtrip_appended_payloads_through_storage_wal(
        appended
            .into_iter()
            .take(pre_commit_record_count)
            .collect::<Vec<_>>(),
    );
    let outcome = recover_payloads_at(
        10,
        roundtripped.iter().map(|(payload, storage_wal_kind_tag)| {
            CatalogDurableMutationPayload::with_storage_wal_kind_tag(payload, *storage_wal_kind_tag)
        }),
    );

    assert!(outcome.report.replayed_batches.is_empty());
    assert_eq!(outcome.report.skipped_incomplete_batches.len(), 1);
    assert_eq!(
        outcome.report.skipped_incomplete_batches[0].reason,
        CatalogSkippedBatchReason::EndOfLogBeforeCommit
    );
    assert_empty_snapshot_at(&outcome, 10);
}

#[test]
fn recovery_ignores_crash_before_commit_payloads_after_storage_wal_frame_roundtrip() {
    let planner = store_at(10);
    let plan = plan_product_batch(&planner, 10, 11);
    let records = plan
        .mutation_plan
        .records()
        .into_iter()
        .take(plan.mutation_plan.record_count() - 1)
        .collect::<Vec<_>>();

    let roundtripped = roundtrip_catalog_records_through_storage_wal(records, 300);
    let outcome = recover_payloads_at(
        10,
        roundtripped.iter().map(|(payload, storage_wal_kind_tag)| {
            CatalogDurableMutationPayload::with_storage_wal_kind_tag(payload, *storage_wal_kind_tag)
        }),
    );

    assert!(outcome.report.replayed_batches.is_empty());
    assert_eq!(outcome.report.skipped_incomplete_batches.len(), 1);
    assert_eq!(
        outcome.report.skipped_incomplete_batches[0].reason,
        CatalogSkippedBatchReason::EndOfLogBeforeCommit
    );
    assert_empty_snapshot_at(&outcome, 10);
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

fn roundtrip_appended_payloads_through_storage_wal(
    appended: Vec<(CatalogMutationRecordKind, Vec<u8>, u64)>,
) -> Vec<(Vec<u8>, u16)> {
    let mut previous_lsn = None;
    appended
        .into_iter()
        .map(|(kind, payload, lsn)| {
            let roundtripped = roundtrip_storage_wal_frame(
                storage_wal_kind_for_catalog_record(kind),
                payload,
                Lsn::new(lsn),
                previous_lsn,
            );
            previous_lsn = Some(roundtripped.header.lsn);
            (
                roundtripped.payload,
                storage_wal_kind_tag_for_catalog_record(kind),
            )
        })
        .collect()
}

fn roundtrip_catalog_records_through_storage_wal(
    records: Vec<CatalogMutationRecord>,
    first_lsn: u64,
) -> Vec<(Vec<u8>, u16)> {
    let mut previous_lsn = None;
    records
        .into_iter()
        .enumerate()
        .map(|(index, record)| {
            let kind = record.kind();
            let lsn = Lsn::new(first_lsn + index as u64);
            let payload = record.encode_durable_payload().unwrap();
            let roundtripped = roundtrip_storage_wal_frame(
                storage_wal_kind_for_catalog_record(kind),
                payload,
                lsn,
                previous_lsn,
            );

            assert_eq!(
                CatalogMutationRecord::decode_durable_payload(roundtripped.payload()).unwrap(),
                record
            );
            previous_lsn = Some(roundtripped.header.lsn);
            (
                roundtripped.payload,
                storage_wal_kind_tag_for_catalog_record(kind),
            )
        })
        .collect()
}

fn roundtrip_storage_wal_frame(
    storage_kind: WalRecordKind,
    payload: Vec<u8>,
    lsn: Lsn,
    previous_lsn: Option<Lsn>,
) -> WalRecord {
    let wal_record = WalRecord::from_parts(
        storage_kind,
        lsn,
        previous_lsn,
        Some(TransactionId::new(77)),
        payload,
    )
    .unwrap();
    let encoded = encode_wal_record(&wal_record).unwrap();
    let (decoded_wal_record, consumed) = decode_wal_record_frame(&encoded).unwrap().unwrap();

    assert_eq!(consumed, encoded.len());
    assert_eq!(decoded_wal_record.header.kind, storage_kind);
    assert_eq!(decoded_wal_record.header.lsn, lsn);
    assert_eq!(decoded_wal_record.header.previous_lsn, previous_lsn);
    assert_eq!(
        decoded_wal_record.header.transaction_id,
        Some(TransactionId::new(77))
    );
    decoded_wal_record
}

fn storage_wal_kind_for_catalog_record(kind: CatalogMutationRecordKind) -> WalRecordKind {
    match kind {
        CatalogMutationRecordKind::CatalogChangeBegin => WalRecordKind::CatalogChangeBegin,
        CatalogMutationRecordKind::CatalogChangeApply => WalRecordKind::CatalogChangeApply,
        CatalogMutationRecordKind::CatalogChangeCommit => WalRecordKind::CatalogChangeCommit,
    }
}

fn storage_wal_kind_tag_for_catalog_record(kind: CatalogMutationRecordKind) -> u16 {
    let storage_kind = storage_wal_kind_for_catalog_record(kind);
    assert_eq!(
        kind.storage_wal_kind_tag() as u64,
        andromeda_storage::wal_record_kind_tag(storage_kind)
    );
    kind.storage_wal_kind_tag()
}
