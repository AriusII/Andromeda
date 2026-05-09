use super::common::*;

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
        andromeda_wal::wal_record_kind_tag(storage_kind)
    );
    kind.storage_wal_kind_tag()
}
