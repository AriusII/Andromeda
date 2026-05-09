use super::common::*;

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
            andromeda_wal::wal_record_kind_tag(storage_kind)
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
        },
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
            },
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
        },
        _ => unreachable!("definition batch WAL sequence must start with Begin"),
    }

    let error = record.encode_durable_payload().unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("source hash"));
}

#[test]
fn catalog_mutation_plan_rejects_zero_direct_identity_fields() {
    let store = store_at(10);
    let plan = plan_product_batch(&store, 10, 11).mutation_plan;

    let zero_batch = CatalogMutationPlan::new(
        DefinitionBatchId::new(0),
        plan.database_id,
        plan.namespace_id,
        plan.previous_version,
        plan.next_version,
        plan.source_hash,
        plan.dependency_graph_hash,
        plan.deltas.clone(),
    )
    .unwrap_err();
    assert_eq!(zero_batch.kind(), AndromedaErrorKind::Catalog);
    assert!(zero_batch.message().contains("batch id"));

    let zero_database = CatalogMutationPlan::new(
        plan.batch_id,
        DatabaseId::new(0),
        plan.namespace_id,
        plan.previous_version,
        plan.next_version,
        plan.source_hash,
        plan.dependency_graph_hash,
        plan.deltas.clone(),
    )
    .unwrap_err();
    assert_eq!(zero_database.kind(), AndromedaErrorKind::Catalog);
    assert!(zero_database.message().contains("database id"));

    let zero_namespace = CatalogMutationPlan::new(
        plan.batch_id,
        plan.database_id,
        NamespaceId::new(0),
        plan.previous_version,
        plan.next_version,
        plan.source_hash,
        plan.dependency_graph_hash,
        plan.deltas,
    )
    .unwrap_err();
    assert_eq!(zero_namespace.kind(), AndromedaErrorKind::Catalog);
    assert!(zero_namespace.message().contains("namespace id"));
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
