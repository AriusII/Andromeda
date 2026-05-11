#![forbid(unsafe_code)]

use andromeda_catalog_recovery::{
    CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH, CatalogDurableMutationPayload,
    CatalogMutationBoundary, CatalogMutationDelta, CatalogMutationOperation, CatalogMutationRecord,
    CatalogPublicationSemantics, CatalogRecoveryAnomalyKind, CatalogRecoveryApplyTarget,
    CatalogRecoveryReport, CatalogSkippedBatchReason, DefinitionBatchDependencyGraphHash,
    decode_catalog_durable_payload, encode_catalog_durable_payload,
    recover_catalog_target_from_durable_payloads, replay_catalog_mutation_records_into_target,
};
use andromeda_catalog_store::{
    CatalogDefinition, CatalogObjectRef, ObjectKind, QualifiedName, TableDefinition,
};
use andromeda_definition_batch::{
    DefinitionBatch, DefinitionBatchId, DefinitionBatchSourceHash, DefinitionOperation,
};
use andromeda_error::AndromedaResult;
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, DatabaseId, NamespaceId, ScalarType,
    TypeDescriptor,
};

const DATABASE_ID: DatabaseId = DatabaseId::new(1);
const NAMESPACE_ID: NamespaceId = NamespaceId::new(2);

#[derive(Debug, Clone, PartialEq, Eq)]
struct DummyTarget {
    database_id: DatabaseId,
    namespace_id: NamespaceId,
    visible_version: CatalogVersion,
    applied_batches: Vec<DefinitionBatchId>,
}

impl CatalogRecoveryApplyTarget for DummyTarget {
    fn recovery_database_id(&self) -> DatabaseId {
        self.database_id
    }

    fn recovery_namespace_id(&self) -> NamespaceId {
        self.namespace_id
    }

    fn recovery_visible_catalog_version(&self) -> CatalogVersion {
        self.visible_version
    }

    fn apply_recovered_catalog_mutation(
        &mut self,
        boundary: &CatalogMutationBoundary,
        _deltas: &[CatalogMutationDelta],
    ) -> AndromedaResult<()> {
        self.validate_recovery_boundary_identity(boundary)?;
        self.visible_version = boundary.next_version;
        self.applied_batches.push(boundary.batch_id);
        Ok(())
    }
}

#[test]
fn recovery_replays_committed_batches_into_generic_target() {
    let batch_one = table_batch(10, &[(1, "Inventory.Product")]);
    let batch_two = table_batch(11, &[(2, "Inventory.Stock")]);
    let records = records_for_batch(&batch_one)
        .into_iter()
        .chain(records_for_batch(&batch_two))
        .collect::<Vec<_>>();

    let outcome = replay_catalog_mutation_records_into_target(target_at(10), records);

    assert_eq!(outcome.report.anomaly_count(), 0);
    assert_eq!(outcome.report.replayed_batches.len(), 2);
    assert_eq!(outcome.report.final_visible_catalog_version, version(12));
    assert!(outcome.report.skipped_incomplete_batches.is_empty());
    assert!(outcome.report.skipped_anomalous_batches.is_empty());
    assert_eq!(outcome.target.visible_version, version(12));
    assert_eq!(
        outcome.target.applied_batches,
        vec![batch_one.batch_id, batch_two.batch_id]
    );
}

#[test]
fn recovery_ignores_incomplete_batch_without_commit() {
    let batch = table_batch(10, &[(1, "Inventory.Product")]);
    let records = records_for_batch(&batch)
        .into_iter()
        .take(2)
        .collect::<Vec<_>>();

    let outcome = replay_catalog_mutation_records_into_target(target_at(10), records);

    assert!(outcome.report.replayed_batches.is_empty());
    assert_skipped_incomplete(
        &outcome.report,
        batch.batch_id,
        10,
        11,
        1,
        CatalogSkippedBatchReason::EndOfLogBeforeCommit,
    );
    assert!(outcome.report.skipped_anomalous_batches.is_empty());
    assert_has_anomaly_for_batch(
        &outcome.report,
        CatalogRecoveryAnomalyKind::MissingApplyRecords,
        batch.batch_id,
    );
    assert_eq!(outcome.report.final_visible_catalog_version, version(10));
    assert_eq!(outcome.target.visible_version, version(10));
}

#[test]
fn recovery_reconstructs_only_last_valid_catalog_version_before_incomplete_tail() {
    let batch_one = table_batch(10, &[(1, "Inventory.Product")]);
    let batch_two = table_batch(11, &[(2, "Inventory.Stock")]);
    let records = records_for_batch(&batch_one)
        .into_iter()
        .chain(records_for_batch(&batch_two).into_iter().take(2))
        .collect::<Vec<_>>();

    let outcome = replay_catalog_mutation_records_into_target(target_at(10), records);

    assert_eq!(outcome.report.anomaly_count(), 1);
    assert_eq!(outcome.report.replayed_batches.len(), 1);
    assert_eq!(
        outcome.report.replayed_batches[0].batch_id,
        batch_one.batch_id
    );
    assert_eq!(
        outcome.report.replayed_batches[0].previous_version,
        version(10)
    );
    assert_eq!(outcome.report.replayed_batches[0].next_version, version(11));
    assert_eq!(outcome.report.replayed_batches[0].applied_delta_count, 1);
    assert_skipped_incomplete(
        &outcome.report,
        batch_two.batch_id,
        11,
        12,
        1,
        CatalogSkippedBatchReason::EndOfLogBeforeCommit,
    );
    assert!(outcome.report.skipped_anomalous_batches.is_empty());
    assert_has_anomaly_for_batch(
        &outcome.report,
        CatalogRecoveryAnomalyKind::MissingApplyRecords,
        batch_two.batch_id,
    );
    assert_eq!(outcome.report.final_visible_catalog_version, version(11));
    assert_eq!(outcome.target.visible_version, version(11));
    assert_eq!(outcome.target.applied_batches, vec![batch_one.batch_id]);
}

#[test]
fn recovery_rejects_committed_batch_missing_tail_apply_record() {
    let batch = table_batch(
        10,
        &[
            (1, "Inventory.Product"),
            (2, "Inventory.Stock"),
            (3, "Inventory.Audit"),
        ],
    );
    let mut records = records_for_batch(&batch);
    records.remove(3);

    let outcome = replay_catalog_mutation_records_into_target(target_at(10), records);

    assert!(outcome.report.replayed_batches.is_empty());
    assert_has_anomaly_for_batch(
        &outcome.report,
        CatalogRecoveryAnomalyKind::ApplyRecordCountMismatch,
        batch.batch_id,
    );
    assert_skipped_incomplete(
        &outcome.report,
        batch.batch_id,
        10,
        11,
        2,
        CatalogSkippedBatchReason::ApplyRecordCountMismatch,
    );
    assert!(outcome.report.skipped_anomalous_batches.is_empty());
    assert_eq!(outcome.report.final_visible_catalog_version, version(10));
    assert_eq!(outcome.target.visible_version, version(10));
}

#[test]
fn recovery_rejects_apply_count_above_replay_limit_before_accumulating_batch() {
    let batch = table_batch(10, &[(1, "Inventory.Product")]);
    let mut records = records_for_batch(&batch);
    match &mut records[0] {
        CatalogMutationRecord::Begin(boundary) => {
            boundary.expected_apply_count = CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH + 1;
        },
        _ => unreachable!("definition batch WAL sequence must start with Begin"),
    }

    let outcome = replay_catalog_mutation_records_into_target(target_at(10), records);

    assert!(outcome.report.replayed_batches.is_empty());
    assert_has_anomaly_for_batch(
        &outcome.report,
        CatalogRecoveryAnomalyKind::ApplyRecordLimitExceeded,
        batch.batch_id,
    );
    assert_skipped_anomalous(
        &outcome.report,
        batch.batch_id,
        10,
        11,
        0,
        CatalogSkippedBatchReason::ApplyRecordLimitExceeded,
    );
    assert!(outcome.report.skipped_incomplete_batches.is_empty());
    assert_eq!(outcome.report.final_visible_catalog_version, version(10));
}

#[test]
fn recovery_reports_commit_and_apply_without_begin() {
    let batch = table_batch(10, &[(1, "Inventory.Product")]);
    let records = records_for_batch(&batch);

    let outcome = replay_catalog_mutation_records_into_target(
        target_at(10),
        vec![records[1].clone(), records[2].clone()],
    );

    assert_eq!(outcome.report.anomaly_count(), 2);
    assert_has_anomaly(
        &outcome.report.anomalies,
        CatalogRecoveryAnomalyKind::ApplyWithoutBegin,
    );
    assert_has_anomaly(
        &outcome.report.anomalies,
        CatalogRecoveryAnomalyKind::CommitWithoutBegin,
    );
    assert_eq!(outcome.target.visible_version, version(10));
}

#[test]
fn recovery_rejects_physically_reordered_apply_records() {
    let batch = table_batch(10, &[(1, "Inventory.Product"), (2, "Inventory.Stock")]);
    let mut records = records_for_batch(&batch);
    records.swap(1, 2);

    let outcome = replay_catalog_mutation_records_into_target(target_at(10), records);

    assert!(outcome.report.replayed_batches.is_empty());
    assert_has_anomaly_for_batch(
        &outcome.report,
        CatalogRecoveryAnomalyKind::ApplyRecordOrderMismatch,
        batch.batch_id,
    );
    assert_skipped_anomalous(
        &outcome.report,
        batch.batch_id,
        10,
        11,
        2,
        CatalogSkippedBatchReason::ApplyRecordOrderMismatch,
    );
    assert!(outcome.report.skipped_incomplete_batches.is_empty());
    assert_eq!(outcome.report.final_visible_catalog_version, version(10));
}

#[test]
fn recovery_rejects_begin_commit_boundary_mismatch_all_or_nothing() {
    let batch = table_batch(10, &[(1, "Inventory.Product")]);
    let mut records = records_for_batch(&batch);
    match records.last_mut().unwrap() {
        CatalogMutationRecord::Commit(boundary) => {
            boundary.next_version = CatalogVersion::new(boundary.next_version.get() + 1);
        },
        _ => unreachable!("definition batch WAL sequence must end with Commit"),
    }

    let outcome = replay_catalog_mutation_records_into_target(target_at(10), records);

    assert!(outcome.report.replayed_batches.is_empty());
    assert_has_anomaly_for_batch(
        &outcome.report,
        CatalogRecoveryAnomalyKind::CommitBoundaryMismatch,
        batch.batch_id,
    );
    assert_skipped_anomalous(
        &outcome.report,
        batch.batch_id,
        10,
        11,
        1,
        CatalogSkippedBatchReason::CommitBoundaryMismatch,
    );
    assert_eq!(outcome.report.final_visible_catalog_version, version(10));
}

#[test]
fn recovery_rejects_definition_batch_hash_mismatch() {
    let batch = table_batch(10, &[(1, "Inventory.Product")]);
    let mut records = records_for_batch(&batch);
    match records.last_mut().unwrap() {
        CatalogMutationRecord::Commit(boundary) => {
            boundary.source_hash = DefinitionBatchSourceHash::new([0x44; 32]);
        },
        _ => unreachable!("definition batch WAL sequence must end with Commit"),
    }

    let outcome = replay_catalog_mutation_records_into_target(target_at(10), records);

    assert!(outcome.report.replayed_batches.is_empty());
    assert_has_anomaly(
        &outcome.report.anomalies,
        CatalogRecoveryAnomalyKind::CommitBoundaryMismatch,
    );
    assert_skipped_anomalous(
        &outcome.report,
        batch.batch_id,
        10,
        11,
        1,
        CatalogSkippedBatchReason::CommitBoundaryMismatch,
    );
    assert_eq!(outcome.report.final_visible_catalog_version, version(10));
}

#[test]
fn recovery_rejects_tampered_apply_with_stale_definition_batch_hashes() {
    let batch = table_batch(10, &[(1, "Inventory.Product")]);
    let mut records = records_for_batch(&batch);
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

    let outcome = replay_catalog_mutation_records_into_target(target_at(10), records);

    assert!(outcome.report.replayed_batches.is_empty());
    assert_has_anomaly_for_batch(
        &outcome.report,
        CatalogRecoveryAnomalyKind::DefinitionBatchHashMismatch,
        batch.batch_id,
    );
    assert_skipped_anomalous(
        &outcome.report,
        batch.batch_id,
        10,
        11,
        1,
        CatalogSkippedBatchReason::DefinitionBatchHashMismatch,
    );
    assert_eq!(outcome.report.final_visible_catalog_version, version(10));
}

#[test]
fn recovery_reports_duplicate_and_sparse_apply_indexes() {
    let batch = table_batch(10, &[(1, "Inventory.Product"), (2, "Inventory.Stock")]);

    let mut duplicate_records = records_for_batch(&batch);
    if let CatalogMutationRecord::Apply(delta) = &mut duplicate_records[2] {
        delta.operation_index = 0;
    }
    let duplicate_outcome =
        replay_catalog_mutation_records_into_target(target_at(10), duplicate_records);
    assert_has_anomaly(
        &duplicate_outcome.report.anomalies,
        CatalogRecoveryAnomalyKind::DuplicateApplyIndex,
    );
    assert_skipped_anomalous(
        &duplicate_outcome.report,
        batch.batch_id,
        10,
        11,
        2,
        CatalogSkippedBatchReason::DuplicateApplyIndex,
    );
    assert_eq!(
        duplicate_outcome.report.final_visible_catalog_version,
        version(10)
    );

    let mut sparse_records = records_for_batch(&batch);
    if let CatalogMutationRecord::Apply(delta) = &mut sparse_records[2] {
        delta.operation_index = 2;
    }
    let sparse_outcome = replay_catalog_mutation_records_into_target(target_at(10), sparse_records);
    assert_has_anomaly(
        &sparse_outcome.report.anomalies,
        CatalogRecoveryAnomalyKind::SparseApplyIndexes,
    );
    assert_skipped_incomplete(
        &sparse_outcome.report,
        batch.batch_id,
        10,
        11,
        2,
        CatalogSkippedBatchReason::SparseApplyIndexes,
    );
    assert_eq!(
        sparse_outcome.report.final_visible_catalog_version,
        version(10)
    );
}

#[test]
fn recovery_reports_wrong_identity_version_gap_outer_kind_and_payload_corruption() {
    let batch = table_batch(10, &[(1, "Inventory.Product")]);
    let records = records_for_batch(&batch);

    let mut wrong_identity_records = records.clone();
    for record in &mut wrong_identity_records {
        match record {
            CatalogMutationRecord::Begin(boundary) | CatalogMutationRecord::Commit(boundary) => {
                boundary.database_id = DatabaseId::new(DATABASE_ID.get() + 100);
            },
            CatalogMutationRecord::Apply(_) => {},
        }
    }
    let wrong_identity_outcome =
        replay_catalog_mutation_records_into_target(target_at(10), wrong_identity_records);
    assert_has_anomaly(
        &wrong_identity_outcome.report.anomalies,
        CatalogRecoveryAnomalyKind::WrongCatalogIdentity,
    );
    assert_skipped_anomalous(
        &wrong_identity_outcome.report,
        batch.batch_id,
        10,
        11,
        1,
        CatalogSkippedBatchReason::WrongCatalogIdentity,
    );
    assert_eq!(
        wrong_identity_outcome.report.final_visible_catalog_version,
        version(10)
    );

    let version_gap_outcome = replay_catalog_mutation_records_into_target(target_at(9), records);
    assert_has_anomaly(
        &version_gap_outcome.report.anomalies,
        CatalogRecoveryAnomalyKind::VersionGap,
    );
    assert_skipped_anomalous(
        &version_gap_outcome.report,
        batch.batch_id,
        10,
        11,
        1,
        CatalogSkippedBatchReason::VersionGap,
    );
    assert_eq!(
        version_gap_outcome.report.final_visible_catalog_version,
        version(9)
    );

    let encoded_begin = encode_catalog_durable_payload(&records_for_batch(&batch)[0]).unwrap();
    let wrong_outer_kind = recover_catalog_target_from_durable_payloads(
        target_at(10),
        vec![CatalogDurableMutationPayload::with_storage_wal_kind_tag(
            &encoded_begin,
            andromeda_catalog_recovery::CatalogMutationRecordKind::CatalogChangeApply
                .storage_wal_kind_tag(),
        )],
    );
    assert_has_anomaly(
        &wrong_outer_kind.report.anomalies,
        CatalogRecoveryAnomalyKind::OuterStorageKindMismatch,
    );

    const VERSION_OFFSET: usize = 8;
    const CHECKSUM_OFFSET: usize = 20;

    let mut magic_mismatch = encoded_begin.clone();
    magic_mismatch[0] ^= 0xFF;

    let mut unsupported_version = encoded_begin.clone();
    unsupported_version[VERSION_OFFSET..VERSION_OFFSET + 2].copy_from_slice(&99_u16.to_le_bytes());

    let mut checksum_mismatch = encoded_begin;
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
        let corrupt_outcome = recover_catalog_target_from_durable_payloads(
            target_at(10),
            vec![CatalogDurableMutationPayload::new(&payload)],
        );
        assert_has_anomaly(&corrupt_outcome.report.anomalies, expected_kind);
        assert_eq!(corrupt_outcome.target.visible_version, version(10));
    }
}

#[test]
fn durable_payload_rejects_planned_only_publication_boundary() {
    let batch = table_batch(10, &[(1, "Inventory.Product")]);
    let mut record = records_for_batch(&batch).into_iter().next().unwrap();
    match &mut record {
        CatalogMutationRecord::Begin(boundary) => {
            boundary.publication_semantics = CatalogPublicationSemantics::PlannedVersionOnly;
        },
        _ => unreachable!("definition batch WAL sequence must start with Begin"),
    }

    let error = encode_catalog_durable_payload(&record).unwrap_err();

    assert!(error.message().contains("durable publication semantics"));
}

#[test]
fn durable_payload_boundaries_roundtrip_definition_batch_hashes() {
    let batch = table_batch(10, &[(1, "Inventory.Product")]);
    let expected_source_hash = batch.source_hash();
    let expected_dependency_graph_hash = batch.dependency_graph_hash().unwrap();
    let records = records_for_batch(&batch);

    for record in [records.first().unwrap(), records.last().unwrap()] {
        let decoded =
            decode_catalog_durable_payload(&encode_catalog_durable_payload(record).unwrap())
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
fn durable_payload_boundary_rejects_missing_definition_batch_hash() {
    let batch = table_batch(10, &[(1, "Inventory.Product")]);
    let mut record = records_for_batch(&batch).into_iter().next().unwrap();
    match &mut record {
        CatalogMutationRecord::Begin(boundary) => {
            boundary.source_hash = DefinitionBatchSourceHash::default();
        },
        _ => unreachable!("definition batch WAL sequence must start with Begin"),
    }

    let source_error = encode_catalog_durable_payload(&record).unwrap_err();
    assert!(source_error.message().contains("source hash"));

    let mut record = records_for_batch(&batch).into_iter().next().unwrap();
    match &mut record {
        CatalogMutationRecord::Begin(boundary) => {
            boundary.dependency_graph_hash = DefinitionBatchDependencyGraphHash::default();
        },
        _ => unreachable!("definition batch WAL sequence must start with Begin"),
    }

    let dependency_error = encode_catalog_durable_payload(&record).unwrap_err();
    assert!(dependency_error.message().contains("dependency graph hash"));
}

/// Audit-evidence gap closure: replaying a committed batch whose
/// `previous_version` equals the target's current version minus one (i.e. the
/// batch was already applied before the replay window) must be silently
/// skipped rather than double-applied.
///
/// Concretely: target is ALREADY at version 11; WAL contains a batch with
/// `previous_version=10 / next_version=11`. The replay engine cannot confirm
/// the batch was applied (it has no applied-batch registry), so it classifies
/// the situation as `VersionGap` and places the batch in
/// `skipped_anomalous_batches`.
///
/// FOLLOWUP: The `VersionGap` classification correctly prevents double-apply
/// but does not distinguish "already applied" from "skipped a version". A
/// dedicated `DuplicateApply` anomaly kind would improve audit observability and
/// allow operators to disambiguate the two scenarios.
#[test]
fn recovery_skips_already_applied_batch_when_target_already_at_next_version() {
    // Batch: previous_version=10, next_version=11.
    let batch = table_batch(10, &[(1, "Inventory.Product")]);
    let records = records_for_batch(&batch);

    // Target is ALREADY at version 11 (the batch's next_version).
    // The replay engine sees: boundary.previous_version(10) ≠ target.visible(11).
    let outcome = replay_catalog_mutation_records_into_target(target_at(11), records);

    // The batch must NOT be re-applied.
    assert!(
        outcome.report.replayed_batches.is_empty(),
        "already-at-next-version batch must not be re-replayed into the target"
    );

    // Classified as VersionGap and placed in skipped_anomalous_batches (not incomplete).
    assert_skipped_anomalous(
        &outcome.report,
        batch.batch_id,
        10, // previous_version from the boundary record
        11, // next_version from the boundary record
        1,  // observed_apply_count: one Apply record was decoded before the check
        CatalogSkippedBatchReason::VersionGap,
    );
    assert!(
        outcome.report.skipped_incomplete_batches.is_empty(),
        "a fully-formed batch must not appear in the incomplete list"
    );

    // Anomaly list must contain a VersionGap record for this batch.
    assert_has_anomaly_for_batch(
        &outcome.report,
        CatalogRecoveryAnomalyKind::VersionGap,
        batch.batch_id,
    );

    // Target version must remain at 11 (unchanged).
    assert_eq!(
        outcome.report.final_visible_catalog_version,
        version(11),
        "replay of an already-applied batch must leave the target version unchanged"
    );
    assert_eq!(
        outcome.target.visible_version,
        version(11),
        "target visible_version must not regress or advance after skipping an already-applied batch"
    );
    assert!(
        outcome.target.applied_batches.is_empty(),
        "no batch must be applied to the target when it is already at the batch next_version"
    );
}

fn records_for_batch(batch: &DefinitionBatch) -> Vec<CatalogMutationRecord> {
    let next_version = CatalogVersion::new(batch.base_version.get() + 1);
    let boundary = CatalogMutationBoundary {
        batch_id: batch.batch_id,
        database_id: batch.database_id,
        namespace_id: batch.namespace_id,
        previous_version: batch.base_version,
        next_version,
        source_hash: batch.source_hash(),
        dependency_graph_hash: batch.dependency_graph_hash().unwrap(),
        expected_apply_count: batch.operations.len(),
        publication_semantics: CatalogPublicationSemantics::DurablePublicationExternal,
    };

    let mut records = Vec::with_capacity(batch.operations.len() + 2);
    records.push(CatalogMutationRecord::Begin(boundary));
    records.extend(
        batch
            .operations
            .iter()
            .enumerate()
            .map(|(operation_index, operation)| match operation {
                DefinitionOperation::Create(definition) => CatalogMutationRecord::Apply(Box::new(
                    CatalogMutationDelta::create(operation_index, next_version, definition.clone()),
                )),
                DefinitionOperation::Deprecate(target) => CatalogMutationRecord::Apply(Box::new(
                    CatalogMutationDelta::deprecate(operation_index, next_version, target.clone()),
                )),
            }),
    );
    records.push(CatalogMutationRecord::Commit(boundary));
    records
}

fn table_batch(base_version: u64, tables: &[(u64, &str)]) -> DefinitionBatch {
    let next_version = version(base_version + 1);
    DefinitionBatch {
        batch_id: DefinitionBatchId::new(base_version + 100),
        database_id: DATABASE_ID,
        namespace_id: NAMESPACE_ID,
        base_version: version(base_version),
        operations: tables
            .iter()
            .map(|(id, name)| {
                DefinitionOperation::Create(table_definition(*id, name, next_version))
            })
            .collect(),
    }
}

fn table_definition(id: u64, name: &str, catalog_version: CatalogVersion) -> CatalogDefinition {
    CatalogDefinition::Table(TableDefinition {
        object: object(id, name, ObjectKind::Table, catalog_version),
        columns: vec![column("ProductId", 0)],
    })
}

fn object(
    id: u64,
    name: &str,
    kind: ObjectKind,
    catalog_version: CatalogVersion,
) -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(id),
        name: QualifiedName::parse(name).unwrap(),
        kind,
        catalog_version,
    }
}

fn target_at(catalog_version: u64) -> DummyTarget {
    DummyTarget {
        database_id: DATABASE_ID,
        namespace_id: NAMESPACE_ID,
        visible_version: version(catalog_version),
        applied_batches: Vec::new(),
    }
}

fn assert_has_anomaly(
    anomalies: &[andromeda_catalog_recovery::CatalogRecoveryAnomaly],
    kind: CatalogRecoveryAnomalyKind,
) {
    assert!(anomalies.iter().any(|anomaly| anomaly.kind == kind));
}

fn assert_has_anomaly_for_batch(
    report: &CatalogRecoveryReport,
    kind: CatalogRecoveryAnomalyKind,
    batch_id: DefinitionBatchId,
) {
    assert!(
        report
            .anomalies
            .iter()
            .any(|anomaly| anomaly.kind == kind && anomaly.batch_id == Some(batch_id)),
        "missing anomaly {kind:?} for batch {batch_id:?}: {:?}",
        report.anomalies
    );
}

fn assert_skipped_incomplete(
    report: &CatalogRecoveryReport,
    batch_id: DefinitionBatchId,
    previous_version: u64,
    next_version: u64,
    observed_apply_count: usize,
    reason: CatalogSkippedBatchReason,
) {
    assert_eq!(report.skipped_incomplete_batches.len(), 1);
    assert_eq!(report.skipped_incomplete_batches[0].batch_id, batch_id);
    assert_eq!(
        report.skipped_incomplete_batches[0].previous_version,
        version(previous_version)
    );
    assert_eq!(
        report.skipped_incomplete_batches[0].next_version,
        version(next_version)
    );
    assert_eq!(
        report.skipped_incomplete_batches[0].observed_apply_count,
        observed_apply_count
    );
    assert_eq!(report.skipped_incomplete_batches[0].reason, reason);
}

fn assert_skipped_anomalous(
    report: &CatalogRecoveryReport,
    batch_id: DefinitionBatchId,
    previous_version: u64,
    next_version: u64,
    observed_apply_count: usize,
    reason: CatalogSkippedBatchReason,
) {
    assert_eq!(report.skipped_anomalous_batches.len(), 1);
    assert_eq!(report.skipped_anomalous_batches[0].batch_id, batch_id);
    assert_eq!(
        report.skipped_anomalous_batches[0].previous_version,
        version(previous_version)
    );
    assert_eq!(
        report.skipped_anomalous_batches[0].next_version,
        version(next_version)
    );
    assert_eq!(
        report.skipped_anomalous_batches[0].observed_apply_count,
        observed_apply_count
    );
    assert_eq!(report.skipped_anomalous_batches[0].reason, reason);
}

fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(ScalarType::I64),
        ordinal,
    }
}

fn version(value: u64) -> CatalogVersion {
    CatalogVersion::new(value)
}
