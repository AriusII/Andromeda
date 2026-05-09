use std::collections::BTreeSet;

use crate::{
    CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH, CatalogMutationBoundary, CatalogMutationDelta,
    CatalogMutationOperation, CatalogMutationPlan, CatalogMutationRecord, CatalogSnapshot,
    DefinitionBatch, DefinitionOperation,
    recovery::{
        CatalogRecoveredBatch, CatalogRecoveryAnomaly, CatalogRecoveryAnomalyKind,
        CatalogRecoveryOutcome, CatalogRecoveryReport, CatalogSkippedBatch,
        CatalogSkippedBatchReason,
    },
};

#[derive(Debug, Clone)]
pub(super) struct IndexedCatalogMutationRecord {
    pub(super) record_index: usize,
    pub(super) record: CatalogMutationRecord,
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

pub(super) fn replay_indexed_catalog_mutation_records(
    mut snapshot: CatalogSnapshot,
    records: Vec<IndexedCatalogMutationRecord>,
    mut anomalies: Vec<CatalogRecoveryAnomaly>,
) -> CatalogRecoveryOutcome {
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
                    &mut snapshot,
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

    let final_visible_catalog_version = snapshot.visible_version();
    CatalogRecoveryOutcome {
        snapshot,
        report: CatalogRecoveryReport {
            replayed_batches,
            skipped_incomplete_batches,
            skipped_anomalous_batches,
            anomalies,
            final_visible_catalog_version,
        },
    }
}

fn replay_committed_batch(
    snapshot: &mut CatalogSnapshot,
    open: PendingCatalogBatch,
    commit_record_index: usize,
    replayed_batches: &mut Vec<CatalogRecoveredBatch>,
    skipped_incomplete_batches: &mut Vec<CatalogSkippedBatch>,
    skipped_anomalous_batches: &mut Vec<CatalogSkippedBatch>,
    anomalies: &mut Vec<CatalogRecoveryAnomaly>,
) {
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

    if open.boundary.database_id != snapshot.database_id
        || open.boundary.namespace_id != snapshot.namespace_id
    {
        anomalies.push(CatalogRecoveryAnomaly {
            record_index: open.begin_record_index,
            batch_id: Some(open.boundary.batch_id),
            kind: CatalogRecoveryAnomalyKind::WrongCatalogIdentity,
            detail: format!(
                "catalog mutation batch targets database {:?}/namespace {:?}, but snapshot is database {:?}/namespace {:?}",
                open.boundary.database_id,
                open.boundary.namespace_id,
                snapshot.database_id,
                snapshot.namespace_id
            ),
        });
        skipped_anomalous_batches
            .push(open.skipped(CatalogSkippedBatchReason::WrongCatalogIdentity));
        return;
    }

    if open.boundary.previous_version != snapshot.version {
        anomalies.push(CatalogRecoveryAnomaly {
            record_index: open.begin_record_index,
            batch_id: Some(open.boundary.batch_id),
            kind: CatalogRecoveryAnomalyKind::VersionGap,
            detail: format!(
                "catalog mutation previous version {:?} does not match recovered snapshot version {:?}",
                open.boundary.previous_version, snapshot.version
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

    let mut plan = match CatalogMutationPlan::new(
        open.boundary.batch_id,
        open.boundary.database_id,
        open.boundary.namespace_id,
        open.boundary.previous_version,
        open.boundary.next_version,
        open.boundary.source_hash,
        open.boundary.dependency_graph_hash,
        deltas,
    ) {
        Ok(plan) => plan,
        Err(error) => {
            anomalies.push(CatalogRecoveryAnomaly {
                record_index: open.begin_record_index,
                batch_id: Some(open.boundary.batch_id),
                kind: CatalogRecoveryAnomalyKind::PlanRejected,
                detail: error.message().to_string(),
            });
            skipped_anomalous_batches.push(open.skipped(CatalogSkippedBatchReason::PlanRejected));
            return;
        },
    };
    plan.publication_semantics = open.boundary.publication_semantics;

    if let Err(error) = snapshot.apply_mutation_plan(&plan) {
        anomalies.push(CatalogRecoveryAnomaly {
            record_index: open.begin_record_index,
            batch_id: Some(open.boundary.batch_id),
            kind: CatalogRecoveryAnomalyKind::ReplayRejected,
            detail: error.message().to_string(),
        });
        skipped_anomalous_batches.push(open.skipped(CatalogSkippedBatchReason::ReplayRejected));
        return;
    }

    // The replayed batch was observed via a Begin/Apply*/Commit triple in
    // the durable catalog mutation log, so its next_version is durably
    // published evidence even though `apply_mutation_plan` itself does not
    // advance the visible/durable version.
    snapshot.mark_durable_version_from_recovery(open.boundary.next_version);

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

fn duplicate_apply_index(deltas: &[(usize, CatalogMutationDelta)]) -> Option<(usize, usize)> {
    let mut seen = BTreeSet::new();
    for (record_index, delta) in deltas {
        if !seen.insert(delta.operation_index) {
            return Some((*record_index, delta.operation_index));
        }
    }
    None
}
