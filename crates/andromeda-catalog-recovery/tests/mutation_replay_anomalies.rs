#![forbid(unsafe_code)]

use andromeda_catalog_recovery::{
    CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH, CatalogDurableMutationPayload,
    CatalogMutationBoundary, CatalogMutationDelta, CatalogMutationOperation, CatalogMutationRecord,
    CatalogPublicationSemantics, CatalogRecoveryAnomalyKind, CatalogRecoveryApplyTarget,
    CatalogSkippedBatchReason, encode_catalog_durable_payload,
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
    assert_eq!(outcome.target.visible_version, version(12));
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
    assert_eq!(outcome.report.skipped_incomplete_batches.len(), 1);
    assert_eq!(
        outcome.report.skipped_incomplete_batches[0].reason,
        CatalogSkippedBatchReason::EndOfLogBeforeCommit
    );
    assert_eq!(outcome.target.visible_version, version(10));
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
    assert_has_anomaly(
        &outcome.report.anomalies,
        CatalogRecoveryAnomalyKind::ApplyRecordCountMismatch,
    );
    assert_eq!(
        outcome.report.skipped_incomplete_batches[0].reason,
        CatalogSkippedBatchReason::ApplyRecordCountMismatch
    );
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
    assert_has_anomaly(
        &outcome.report.anomalies,
        CatalogRecoveryAnomalyKind::ApplyRecordLimitExceeded,
    );
    assert_eq!(
        outcome.report.skipped_anomalous_batches[0].reason,
        CatalogSkippedBatchReason::ApplyRecordLimitExceeded
    );
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
    assert_has_anomaly(
        &outcome.report.anomalies,
        CatalogRecoveryAnomalyKind::ApplyRecordOrderMismatch,
    );
    assert_eq!(
        outcome.report.skipped_anomalous_batches[0].reason,
        CatalogSkippedBatchReason::ApplyRecordOrderMismatch
    );
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
    assert_has_anomaly(
        &outcome.report.anomalies,
        CatalogRecoveryAnomalyKind::CommitBoundaryMismatch,
    );
    assert_eq!(
        outcome.report.skipped_anomalous_batches[0].reason,
        CatalogSkippedBatchReason::CommitBoundaryMismatch
    );
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
    assert_has_anomaly(
        &outcome.report.anomalies,
        CatalogRecoveryAnomalyKind::DefinitionBatchHashMismatch,
    );
    assert_eq!(
        outcome.report.skipped_anomalous_batches[0].reason,
        CatalogSkippedBatchReason::DefinitionBatchHashMismatch
    );
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
    assert_eq!(
        duplicate_outcome.report.skipped_anomalous_batches[0].reason,
        CatalogSkippedBatchReason::DuplicateApplyIndex
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
    assert_eq!(
        sparse_outcome.report.skipped_incomplete_batches[0].reason,
        CatalogSkippedBatchReason::SparseApplyIndexes
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

    let version_gap_outcome = replay_catalog_mutation_records_into_target(target_at(9), records);
    assert_has_anomaly(
        &version_gap_outcome.report.anomalies,
        CatalogRecoveryAnomalyKind::VersionGap,
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
