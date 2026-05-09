use std::collections::BTreeSet;

use andromeda_catalog_store::CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH;
use andromeda_definition_batch::{DefinitionBatch, DefinitionOperation};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{
    CatalogDurableMutationPayload, CatalogMutationBoundary, CatalogMutationDelta,
    CatalogMutationOperation, CatalogMutationRecord, CatalogRecoveredBatch, CatalogRecoveryAnomaly,
    CatalogRecoveryAnomalyKind, CatalogRecoveryApplyTarget, CatalogRecoveryReport,
    CatalogSkippedBatch, CatalogSkippedBatchReason, decode_catalog_durable_payload,
    recovery_anomaly_kind_for_decode_error,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRecoveryTargetOutcome<Target> {
    pub target: Target,
    pub report: CatalogRecoveryReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedCatalogMutationRecord {
    pub record_index: usize,
    pub record: CatalogMutationRecord,
}

#[derive(Debug, Clone)]
struct PendingCatalogBatch {
    begin_record_index: usize,
    boundary: CatalogMutationBoundary,
    deltas: Vec<(usize, CatalogMutationDelta)>,
}

impl PendingCatalogBatch {
    fn skipped(&self, reason: CatalogSkippedBatchReason) -> CatalogSkippedBatch {
        CatalogSkippedBatch {
            batch_id: self.boundary.batch_id,
            previous_version: self.boundary.previous_version,
            next_version: self.boundary.next_version,
            observed_apply_count: self.deltas.len(),
            reason,
        }
    }
}

pub fn replay_catalog_mutation_records_into_target<Target>(
    target: Target,
    records: impl IntoIterator<Item = CatalogMutationRecord>,
) -> CatalogRecoveryTargetOutcome<Target>
where
    Target: CatalogRecoveryApplyTarget,
{
    replay_indexed_catalog_mutation_records_into_target(
        target,
        records
            .into_iter()
            .enumerate()
            .map(|(record_index, record)| IndexedCatalogMutationRecord {
                record_index,
                record,
            }),
        Vec::new(),
    )
}

/// Decode durable catalog payloads and replay committed mutation batches into a
/// recovery target.
///
/// Decode failures, optional outer storage-WAL kind mismatches, incomplete
/// batches, and replay anomalies are accumulated in the returned report. Only
/// complete, boundary-consistent batches are applied to the target.
pub fn recover_catalog_target_from_durable_payloads<'a, Target>(
    target: Target,
    payloads: impl IntoIterator<Item = CatalogDurableMutationPayload<'a>>,
) -> CatalogRecoveryTargetOutcome<Target>
where
    Target: CatalogRecoveryApplyTarget,
{
    let mut records = Vec::new();
    let mut anomalies = Vec::new();

    for (record_index, input) in payloads.into_iter().enumerate() {
        match decode_catalog_durable_payload(input.payload) {
            Ok(record) => {
                if let Some(outer_tag) = input.storage_wal_kind_tag {
                    let inner_tag = record.kind().storage_wal_kind_tag();
                    if outer_tag != inner_tag {
                        anomalies.push(CatalogRecoveryAnomaly {
                            record_index,
                            batch_id: boundary_batch_id(&record),
                            kind: CatalogRecoveryAnomalyKind::OuterStorageKindMismatch,
                            detail: format!(
                                "outer storage WAL kind tag {outer_tag} does not match inner catalog payload kind tag {inner_tag}",
                            ),
                        });
                        continue;
                    }
                }

                records.push(IndexedCatalogMutationRecord {
                    record_index,
                    record,
                });
            },
            Err(error) => {
                anomalies.push(CatalogRecoveryAnomaly {
                    record_index,
                    batch_id: None,
                    kind: recovery_anomaly_kind_for_decode_error(error.kind()),
                    detail: format!("{}: {}", error.kind().stable_code(), error.detail()),
                });
            },
        }
    }

    replay_indexed_catalog_mutation_records_into_target(target, records, anomalies)
}

pub fn replay_indexed_catalog_mutation_records_into_target<Target>(
    mut target: Target,
    records: impl IntoIterator<Item = IndexedCatalogMutationRecord>,
    mut anomalies: Vec<CatalogRecoveryAnomaly>,
) -> CatalogRecoveryTargetOutcome<Target>
where
    Target: CatalogRecoveryApplyTarget,
{
    let mut replayed_batches = Vec::new();
    let mut skipped_incomplete_batches = Vec::new();
    let mut skipped_anomalous_batches = Vec::new();
    let mut pending: Option<PendingCatalogBatch> = None;

    for indexed in records {
        match indexed.record {
            CatalogMutationRecord::Begin(boundary) => {
                if let Some(open) = pending.take() {
                    anomalies.push(CatalogRecoveryAnomaly {
                        record_index: indexed.record_index,
                        batch_id: Some(open.boundary.batch_id),
                        kind: CatalogRecoveryAnomalyKind::BeginWhileBatchOpen,
                        detail:
                            "new catalog begin record encountered before the open batch committed"
                                .to_string(),
                    });
                    skipped_incomplete_batches.push(
                        open.skipped(CatalogSkippedBatchReason::BeginSupersededByAnotherBegin),
                    );
                }

                if boundary.expected_apply_count > CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH {
                    anomalies.push(CatalogRecoveryAnomaly {
                        record_index: indexed.record_index,
                        batch_id: Some(boundary.batch_id),
                        kind: CatalogRecoveryAnomalyKind::ApplyRecordLimitExceeded,
                        detail: format!(
                            "catalog mutation batch expected {} apply records, above replay limit {CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH}",
                            boundary.expected_apply_count
                        ),
                    });
                    skipped_anomalous_batches.push(CatalogSkippedBatch {
                        batch_id: boundary.batch_id,
                        previous_version: boundary.previous_version,
                        next_version: boundary.next_version,
                        observed_apply_count: 0,
                        reason: CatalogSkippedBatchReason::ApplyRecordLimitExceeded,
                    });
                    continue;
                }

                pending = Some(PendingCatalogBatch {
                    begin_record_index: indexed.record_index,
                    boundary,
                    deltas: Vec::new(),
                });
            },
            CatalogMutationRecord::Apply(delta) => {
                let exceeds_declared_apply_count = match pending.as_ref() {
                    Some(open) => open.deltas.len() >= open.boundary.expected_apply_count,
                    None => false,
                };

                if exceeds_declared_apply_count {
                    if let Some(open) = pending.take() {
                        anomalies.push(CatalogRecoveryAnomaly {
                            record_index: indexed.record_index,
                            batch_id: Some(open.boundary.batch_id),
                            kind: CatalogRecoveryAnomalyKind::ApplyRecordCountMismatch,
                            detail: format!(
                                "catalog mutation batch received more than {} declared apply records",
                                open.boundary.expected_apply_count
                            ),
                        });
                        skipped_anomalous_batches.push(
                            open.skipped(CatalogSkippedBatchReason::ApplyRecordCountMismatch),
                        );
                    }
                    continue;
                }

                if let Some(open) = pending.as_mut() {
                    open.deltas.push((indexed.record_index, *delta));
                } else {
                    anomalies.push(CatalogRecoveryAnomaly {
                        record_index: indexed.record_index,
                        batch_id: None,
                        kind: CatalogRecoveryAnomalyKind::ApplyWithoutBegin,
                        detail: "catalog apply record has no preceding begin record".to_string(),
                    });
                }
            },
            CatalogMutationRecord::Commit(commit_boundary) => {
                let Some(open) = pending.take() else {
                    anomalies.push(CatalogRecoveryAnomaly {
                        record_index: indexed.record_index,
                        batch_id: Some(commit_boundary.batch_id),
                        kind: CatalogRecoveryAnomalyKind::CommitWithoutBegin,
                        detail: "catalog commit record has no preceding begin record".to_string(),
                    });
                    continue;
                };

                if open.boundary != commit_boundary {
                    anomalies.push(CatalogRecoveryAnomaly {
                        record_index: indexed.record_index,
                        batch_id: Some(commit_boundary.batch_id),
                        kind: CatalogRecoveryAnomalyKind::CommitBoundaryMismatch,
                        detail: "catalog commit boundary does not match its begin boundary"
                            .to_string(),
                    });
                    skipped_anomalous_batches
                        .push(open.skipped(CatalogSkippedBatchReason::CommitBoundaryMismatch));
                    continue;
                }

                replay_committed_batch(
                    &mut target,
                    open,
                    indexed.record_index,
                    &mut replayed_batches,
                    &mut skipped_incomplete_batches,
                    &mut skipped_anomalous_batches,
                    &mut anomalies,
                );
            },
        }
    }

    if let Some(open) = pending {
        anomalies.push(CatalogRecoveryAnomaly {
            record_index: open.begin_record_index,
            batch_id: Some(open.boundary.batch_id),
            kind: CatalogRecoveryAnomalyKind::MissingApplyRecords,
            detail: "catalog mutation batch reached end of log before commit".to_string(),
        });
        skipped_incomplete_batches
            .push(open.skipped(CatalogSkippedBatchReason::EndOfLogBeforeCommit));
    }

    let final_visible_catalog_version = target.recovery_visible_catalog_version();
    CatalogRecoveryTargetOutcome {
        target,
        report: CatalogRecoveryReport {
            replayed_batches,
            skipped_incomplete_batches,
            skipped_anomalous_batches,
            anomalies,
            final_visible_catalog_version,
        },
    }
}

fn boundary_batch_id(
    record: &CatalogMutationRecord,
) -> Option<andromeda_definition_batch::DefinitionBatchId> {
    match record {
        CatalogMutationRecord::Begin(boundary) | CatalogMutationRecord::Commit(boundary) => {
            Some(boundary.batch_id)
        },
        CatalogMutationRecord::Apply(_) => None,
    }
}

fn replay_committed_batch<Target>(
    target: &mut Target,
    open: PendingCatalogBatch,
    commit_record_index: usize,
    replayed_batches: &mut Vec<CatalogRecoveredBatch>,
    skipped_incomplete_batches: &mut Vec<CatalogSkippedBatch>,
    skipped_anomalous_batches: &mut Vec<CatalogSkippedBatch>,
    anomalies: &mut Vec<CatalogRecoveryAnomaly>,
) where
    Target: CatalogRecoveryApplyTarget,
{
    if open.deltas.is_empty() {
        anomalies.push(CatalogRecoveryAnomaly {
            record_index: commit_record_index,
            batch_id: Some(open.boundary.batch_id),
            kind: CatalogRecoveryAnomalyKind::MissingApplyRecords,
            detail: "committed catalog mutation batch contains no apply records".to_string(),
        });
        skipped_incomplete_batches
            .push(open.skipped(CatalogSkippedBatchReason::MissingApplyRecords));
        return;
    }

    if let Some((record_index, duplicate_index)) = duplicate_apply_index(&open.deltas) {
        anomalies.push(CatalogRecoveryAnomaly {
            record_index,
            batch_id: Some(open.boundary.batch_id),
            kind: CatalogRecoveryAnomalyKind::DuplicateApplyIndex,
            detail: format!("duplicate catalog apply operation index {duplicate_index}"),
        });
        skipped_anomalous_batches
            .push(open.skipped(CatalogSkippedBatchReason::DuplicateApplyIndex));
        return;
    }

    if open.deltas.len() != open.boundary.expected_apply_count {
        anomalies.push(CatalogRecoveryAnomaly {
            record_index: commit_record_index,
            batch_id: Some(open.boundary.batch_id),
            kind: CatalogRecoveryAnomalyKind::ApplyRecordCountMismatch,
            detail: format!(
                "catalog mutation batch expected {} apply records, observed {}",
                open.boundary.expected_apply_count,
                open.deltas.len()
            ),
        });
        skipped_incomplete_batches
            .push(open.skipped(CatalogSkippedBatchReason::ApplyRecordCountMismatch));
        return;
    }

    let mut sorted_deltas = open.deltas.clone();
    sorted_deltas.sort_by_key(|(_, delta)| delta.operation_index);
    for (expected_index, (record_index, delta)) in sorted_deltas.iter().enumerate() {
        if delta.operation_index != expected_index {
            anomalies.push(CatalogRecoveryAnomaly {
                record_index: *record_index,
                batch_id: Some(open.boundary.batch_id),
                kind: CatalogRecoveryAnomalyKind::SparseApplyIndexes,
                detail: format!(
                    "catalog apply indexes are sparse: expected {expected_index}, observed {}",
                    delta.operation_index
                ),
            });
            skipped_incomplete_batches
                .push(open.skipped(CatalogSkippedBatchReason::SparseApplyIndexes));
            return;
        }
    }

    for (expected_index, (record_index, delta)) in open.deltas.iter().enumerate() {
        if delta.operation_index != expected_index {
            anomalies.push(CatalogRecoveryAnomaly {
                record_index: *record_index,
                batch_id: Some(open.boundary.batch_id),
                kind: CatalogRecoveryAnomalyKind::ApplyRecordOrderMismatch,
                detail: format!(
                    "catalog apply record physical order mismatch: expected operation index {expected_index}, observed {}",
                    delta.operation_index
                ),
            });
            skipped_anomalous_batches
                .push(open.skipped(CatalogSkippedBatchReason::ApplyRecordOrderMismatch));
            return;
        }
    }

    if open.boundary.database_id != target.recovery_database_id()
        || open.boundary.namespace_id != target.recovery_namespace_id()
    {
        anomalies.push(CatalogRecoveryAnomaly {
            record_index: open.begin_record_index,
            batch_id: Some(open.boundary.batch_id),
            kind: CatalogRecoveryAnomalyKind::WrongCatalogIdentity,
            detail: "catalog mutation batch targets a different recovery target identity"
                .to_string(),
        });
        skipped_anomalous_batches
            .push(open.skipped(CatalogSkippedBatchReason::WrongCatalogIdentity));
        return;
    }

    if open.boundary.previous_version != target.recovery_visible_catalog_version() {
        anomalies.push(CatalogRecoveryAnomaly {
            record_index: open.begin_record_index,
            batch_id: Some(open.boundary.batch_id),
            kind: CatalogRecoveryAnomalyKind::VersionGap,
            detail: format!(
                "catalog mutation previous version {:?} does not match recovered target visible version {:?}",
                open.boundary.previous_version,
                target.recovery_visible_catalog_version()
            ),
        });
        skipped_anomalous_batches.push(open.skipped(CatalogSkippedBatchReason::VersionGap));
        return;
    }

    if open.boundary.next_version.get() != open.boundary.previous_version.get().saturating_add(1) {
        anomalies.push(CatalogRecoveryAnomaly {
            record_index: open.begin_record_index,
            batch_id: Some(open.boundary.batch_id),
            kind: CatalogRecoveryAnomalyKind::VersionGap,
            detail: format!(
                "catalog mutation next version {:?} is not the contiguous successor of previous version {:?}",
                open.boundary.next_version, open.boundary.previous_version
            ),
        });
        skipped_anomalous_batches.push(open.skipped(CatalogSkippedBatchReason::VersionGap));
        return;
    }

    let deltas = sorted_deltas
        .into_iter()
        .map(|(_, delta)| delta)
        .collect::<Vec<_>>();
    let applied_delta_count = deltas.len();

    let reconstructed_batch = definition_batch_from_recovered_deltas(&open.boundary, &deltas);
    let recovered_source_hash = reconstructed_batch.source_hash();
    if recovered_source_hash != open.boundary.source_hash {
        anomalies.push(CatalogRecoveryAnomaly {
            record_index: open.begin_record_index,
            batch_id: Some(open.boundary.batch_id),
            kind: CatalogRecoveryAnomalyKind::DefinitionBatchHashMismatch,
            detail: "recovered DefinitionBatch source hash does not match durable boundary"
                .to_string(),
        });
        skipped_anomalous_batches
            .push(open.skipped(CatalogSkippedBatchReason::DefinitionBatchHashMismatch));
        return;
    }

    let recovered_dependency_graph_hash = match reconstructed_batch.dependency_graph_hash() {
        Ok(hash) => hash,
        Err(error) => {
            anomalies.push(CatalogRecoveryAnomaly {
                record_index: open.begin_record_index,
                batch_id: Some(open.boundary.batch_id),
                kind: CatalogRecoveryAnomalyKind::DefinitionBatchHashMismatch,
                detail: error.message().to_string(),
            });
            skipped_anomalous_batches
                .push(open.skipped(CatalogSkippedBatchReason::DefinitionBatchHashMismatch));
            return;
        },
    };
    if recovered_dependency_graph_hash != open.boundary.dependency_graph_hash {
        anomalies.push(CatalogRecoveryAnomaly {
            record_index: open.begin_record_index,
            batch_id: Some(open.boundary.batch_id),
            kind: CatalogRecoveryAnomalyKind::DefinitionBatchHashMismatch,
            detail:
                "recovered DefinitionBatch dependency graph hash does not match durable boundary"
                    .to_string(),
        });
        skipped_anomalous_batches
            .push(open.skipped(CatalogSkippedBatchReason::DefinitionBatchHashMismatch));
        return;
    }

    if let Err(error) = validate_recovered_plan_shape(&open.boundary, &deltas) {
        anomalies.push(CatalogRecoveryAnomaly {
            record_index: open.begin_record_index,
            batch_id: Some(open.boundary.batch_id),
            kind: CatalogRecoveryAnomalyKind::PlanRejected,
            detail: error.message().to_string(),
        });
        skipped_anomalous_batches.push(open.skipped(CatalogSkippedBatchReason::PlanRejected));
        return;
    }

    if let Err(error) = target.apply_recovered_catalog_mutation(&open.boundary, &deltas) {
        anomalies.push(CatalogRecoveryAnomaly {
            record_index: open.begin_record_index,
            batch_id: Some(open.boundary.batch_id),
            kind: CatalogRecoveryAnomalyKind::ReplayRejected,
            detail: error.message().to_string(),
        });
        skipped_anomalous_batches.push(open.skipped(CatalogSkippedBatchReason::ReplayRejected));
        return;
    }

    replayed_batches.push(CatalogRecoveredBatch {
        batch_id: open.boundary.batch_id,
        previous_version: open.boundary.previous_version,
        next_version: open.boundary.next_version,
        source_hash: open.boundary.source_hash,
        dependency_graph_hash: open.boundary.dependency_graph_hash,
        applied_delta_count,
    });
}

fn definition_batch_from_recovered_deltas(
    boundary: &CatalogMutationBoundary,
    deltas: &[CatalogMutationDelta],
) -> DefinitionBatch {
    let operations = deltas
        .iter()
        .map(|delta| match &delta.operation {
            CatalogMutationOperation::CreateObject { definition, .. } => {
                DefinitionOperation::Create(definition.clone())
            },
            CatalogMutationOperation::DeprecateObject { target } => {
                DefinitionOperation::Deprecate(target.clone())
            },
        })
        .collect();

    DefinitionBatch {
        batch_id: boundary.batch_id,
        database_id: boundary.database_id,
        namespace_id: boundary.namespace_id,
        base_version: boundary.previous_version,
        operations,
    }
}

fn validate_recovered_plan_shape(
    boundary: &CatalogMutationBoundary,
    deltas: &[CatalogMutationDelta],
) -> AndromedaResult<()> {
    if deltas.is_empty() {
        return catalog_recovery_error("catalog mutation plan must contain at least one delta");
    }
    let mut object_ids = BTreeSet::new();
    let mut object_names = BTreeSet::new();
    for (expected_index, delta) in deltas.iter().enumerate() {
        if delta.operation_index != expected_index {
            return catalog_recovery_error(
                "catalog mutation deltas must be dense and operation ordered",
            );
        }
        if delta.planned_version != boundary.next_version {
            return catalog_recovery_error(
                "catalog mutation delta version must match the planned next catalog version",
            );
        }

        match &delta.operation {
            CatalogMutationOperation::CreateObject { object, definition } => {
                definition.validate()?;
                if object.catalog_version != boundary.next_version {
                    return catalog_recovery_error(
                        "catalog mutation object version must match the planned next catalog version",
                    );
                }
                if !object_ids.insert(object.object_id) {
                    return catalog_recovery_error(
                        "catalog mutation plan must not change the same object id twice",
                    );
                }
                if !object_names.insert(object.name.clone()) {
                    return catalog_recovery_error(
                        "catalog mutation plan must not change the same object name twice",
                    );
                }
            },
            CatalogMutationOperation::DeprecateObject { target } => {
                target.validate()?;
                if target.object.catalog_version > boundary.previous_version {
                    return catalog_recovery_error(
                        "catalog mutation lifecycle target version must not be newer than the previous catalog version",
                    );
                }
                if !object_ids.insert(target.object.object_id) {
                    return catalog_recovery_error(
                        "catalog mutation plan must not change the same object id twice",
                    );
                }
                if !object_names.insert(target.object.name.clone()) {
                    return catalog_recovery_error(
                        "catalog mutation plan must not change the same object name twice",
                    );
                }
            },
        }
    }
    Ok(())
}

fn duplicate_apply_index(deltas: &[(usize, CatalogMutationDelta)]) -> Option<(usize, usize)> {
    let mut seen = BTreeSet::new();
    for (record_index, delta) in deltas {
        if !seen.insert(delta.operation_index) {
            return Some((*record_index, delta.operation_index));
        }
    }
    None
}

fn catalog_recovery_error<T>(message: impl Into<String>) -> AndromedaResult<T> {
    Err(AndromedaError::new(AndromedaErrorKind::Catalog, message))
}

#[cfg(test)]
mod tests {
    use andromeda_catalog_store::{
        CatalogDefinition, CatalogObjectRef, ObjectKind, QualifiedName, TableDefinition,
    };
    use andromeda_definition_batch::{
        DefinitionBatchId, DefinitionBatchSourceHash, DefinitionOperation,
    };
    use andromeda_error::AndromedaErrorKind;
    use andromeda_types::{
        CatalogObjectId, CatalogVersion, ColumnDescriptor, DatabaseId, NamespaceId, ScalarType,
        TypeDescriptor,
    };

    use super::*;
    use crate::{CatalogPublicationSemantics, DefinitionBatchDependencyGraphHash};

    const DATABASE_ID: DatabaseId = DatabaseId::new(11);
    const NAMESPACE_ID: NamespaceId = NamespaceId::new(22);

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct DummyTarget {
        database_id: DatabaseId,
        namespace_id: NamespaceId,
        visible_version: CatalogVersion,
        applied: Vec<CatalogMutationBoundary>,
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
            self.applied.push(*boundary);
            Ok(())
        }
    }

    #[test]
    fn generic_replay_applies_committed_batch_into_target_without_snapshot() {
        let (boundary, delta) = recovered_table_batch(1, 2);
        let target = target_at(1);

        let outcome = replay_catalog_mutation_records_into_target(
            target,
            vec![
                CatalogMutationRecord::Begin(boundary),
                CatalogMutationRecord::Apply(Box::new(delta)),
                CatalogMutationRecord::Commit(boundary),
            ],
        );

        assert_eq!(outcome.report.anomaly_count(), 0);
        assert_eq!(outcome.report.replayed_batches.len(), 1);
        assert_eq!(outcome.report.final_visible_catalog_version, version(2));
        assert_eq!(outcome.target.visible_version, version(2));
        assert_eq!(outcome.target.applied, vec![boundary]);
    }

    #[test]
    fn generic_replay_rejects_version_gap_before_target_apply() {
        let (boundary, delta) = recovered_table_batch(1, 2);
        let target = target_at(0);

        let outcome = replay_catalog_mutation_records_into_target(
            target,
            vec![
                CatalogMutationRecord::Begin(boundary),
                CatalogMutationRecord::Apply(Box::new(delta)),
                CatalogMutationRecord::Commit(boundary),
            ],
        );

        assert!(outcome.report.replayed_batches.is_empty());
        assert_eq!(outcome.target.visible_version, version(0));
        assert!(outcome.target.applied.is_empty());
        assert_eq!(
            outcome.report.skipped_anomalous_batches[0].reason,
            CatalogSkippedBatchReason::VersionGap
        );
        assert!(
            outcome
                .report
                .anomalies
                .iter()
                .any(|anomaly| anomaly.kind == CatalogRecoveryAnomalyKind::VersionGap)
        );
    }

    #[test]
    fn generic_replay_classifies_target_rejection_as_replay_rejected() {
        #[derive(Debug, Clone, PartialEq, Eq)]
        struct RejectingTarget(DummyTarget);

        impl CatalogRecoveryApplyTarget for RejectingTarget {
            fn recovery_database_id(&self) -> DatabaseId {
                self.0.recovery_database_id()
            }

            fn recovery_namespace_id(&self) -> NamespaceId {
                self.0.recovery_namespace_id()
            }

            fn recovery_visible_catalog_version(&self) -> CatalogVersion {
                self.0.recovery_visible_catalog_version()
            }

            fn apply_recovered_catalog_mutation(
                &mut self,
                _boundary: &CatalogMutationBoundary,
                _deltas: &[CatalogMutationDelta],
            ) -> AndromedaResult<()> {
                Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "target rejected recovered mutation",
                ))
            }
        }

        let (boundary, delta) = recovered_table_batch(1, 2);
        let outcome = replay_catalog_mutation_records_into_target(
            RejectingTarget(target_at(1)),
            vec![
                CatalogMutationRecord::Begin(boundary),
                CatalogMutationRecord::Apply(Box::new(delta)),
                CatalogMutationRecord::Commit(boundary),
            ],
        );

        assert!(outcome.report.replayed_batches.is_empty());
        assert_eq!(
            outcome.report.skipped_anomalous_batches[0].reason,
            CatalogSkippedBatchReason::ReplayRejected
        );
    }

    fn target_at(visible_version: u64) -> DummyTarget {
        DummyTarget {
            database_id: DATABASE_ID,
            namespace_id: NAMESPACE_ID,
            visible_version: version(visible_version),
            applied: Vec::new(),
        }
    }

    fn recovered_table_batch(
        previous_version: u64,
        next_version: u64,
    ) -> (CatalogMutationBoundary, CatalogMutationDelta) {
        let definition = CatalogDefinition::Table(TableDefinition {
            object: object(100, "Inventory.Product", ObjectKind::Table, next_version),
            columns: vec![ColumnDescriptor {
                name: "ProductId".to_string(),
                data_type: TypeDescriptor::required(ScalarType::I64),
                ordinal: 0,
            }],
        });
        let batch = DefinitionBatch {
            batch_id: DefinitionBatchId::new(900),
            database_id: DATABASE_ID,
            namespace_id: NAMESPACE_ID,
            base_version: version(previous_version),
            operations: vec![DefinitionOperation::Create(definition.clone())],
        };
        let boundary = CatalogMutationBoundary {
            batch_id: batch.batch_id,
            database_id: DATABASE_ID,
            namespace_id: NAMESPACE_ID,
            previous_version: version(previous_version),
            next_version: version(next_version),
            source_hash: DefinitionBatchSourceHash::new(batch.source_hash().as_bytes()),
            dependency_graph_hash: DefinitionBatchDependencyGraphHash::new(
                batch.dependency_graph_hash().unwrap().as_bytes(),
            ),
            expected_apply_count: 1,
            publication_semantics: CatalogPublicationSemantics::DurablePublicationExternal,
        };
        let object = definition.object_ref().clone();
        let delta = CatalogMutationDelta {
            operation_index: 0,
            planned_version: version(next_version),
            operation: CatalogMutationOperation::CreateObject { object, definition },
        };
        (boundary, delta)
    }

    fn object(id: u64, name: &str, kind: ObjectKind, catalog_version: u64) -> CatalogObjectRef {
        CatalogObjectRef {
            object_id: CatalogObjectId::new(id),
            name: QualifiedName::parse(name).unwrap(),
            kind,
            catalog_version: version(catalog_version),
        }
    }

    fn version(value: u64) -> CatalogVersion {
        CatalogVersion::new(value)
    }
}
