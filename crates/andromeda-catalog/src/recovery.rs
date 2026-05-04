//! Catalog mutation recovery replay.
//!
//! Recovery consumes the durable catalog mutation payloads produced by
//! [`crate::CatalogMutationRecord::encode_durable_payload`], reconstructs only
//! fully committed begin/apply/commit batches, and replays those batches into a
//! [`crate::CatalogSnapshot`].  Incomplete or anomalous batches are not applied.

use andromeda_core::CatalogVersion;
use std::collections::BTreeSet;

use crate::{
    CatalogMutationBoundary, CatalogMutationDelta, CatalogMutationPlan, CatalogMutationRecord,
    CatalogSnapshot, DefinitionBatchId,
};

/// Durable catalog mutation payload plus optional outer storage-WAL kind tag.
///
/// When `storage_wal_kind_tag` is present, recovery enforces that the outer
/// storage WAL record kind matches the inner catalog payload kind before the
/// record can participate in replay.
#[derive(Debug, Clone, Copy)]
pub struct CatalogDurableMutationPayload<'a> {
    pub payload: &'a [u8],
    pub storage_wal_kind_tag: Option<u16>,
}

impl<'a> CatalogDurableMutationPayload<'a> {
    pub const fn new(payload: &'a [u8]) -> Self {
        Self {
            payload,
            storage_wal_kind_tag: None,
        }
    }

    pub const fn with_storage_wal_kind_tag(payload: &'a [u8], storage_wal_kind_tag: u16) -> Self {
        Self {
            payload,
            storage_wal_kind_tag: Some(storage_wal_kind_tag),
        }
    }
}

/// Result of catalog recovery replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRecoveryOutcome {
    pub snapshot: CatalogSnapshot,
    pub report: CatalogRecoveryReport,
}

/// Replay report for catalog mutation recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRecoveryReport {
    pub replayed_batches: Vec<CatalogRecoveredBatch>,
    pub skipped_incomplete_batches: Vec<CatalogSkippedBatch>,
    pub skipped_anomalous_batches: Vec<CatalogSkippedBatch>,
    pub anomalies: Vec<CatalogRecoveryAnomaly>,
    pub final_visible_catalog_version: CatalogVersion,
}

impl CatalogRecoveryReport {
    pub fn anomaly_count(&self) -> usize {
        self.anomalies.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRecoveredBatch {
    pub batch_id: DefinitionBatchId,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub applied_delta_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSkippedBatch {
    pub batch_id: DefinitionBatchId,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub observed_apply_count: usize,
    pub reason: CatalogSkippedBatchReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSkippedBatchReason {
    BeginSupersededByAnotherBegin,
    EndOfLogBeforeCommit,
    CommitBoundaryMismatch,
    MissingApplyRecords,
    DuplicateApplyIndex,
    SparseApplyIndexes,
    WrongCatalogIdentity,
    VersionGap,
    PlanRejected,
    ReplayRejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRecoveryAnomaly {
    pub record_index: usize,
    pub batch_id: Option<DefinitionBatchId>,
    pub kind: CatalogRecoveryAnomalyKind,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogRecoveryAnomalyKind {
    PayloadCorruption,
    WrongKindTag,
    OuterStorageKindMismatch,
    CommitWithoutBegin,
    ApplyWithoutBegin,
    BeginWhileBatchOpen,
    CommitBoundaryMismatch,
    WrongCatalogIdentity,
    VersionGap,
    MissingApplyRecords,
    DuplicateApplyIndex,
    SparseApplyIndexes,
    PlanRejected,
    ReplayRejected,
}

#[derive(Debug, Clone)]
struct IndexedCatalogMutationRecord {
    record_index: usize,
    record: CatalogMutationRecord,
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

/// Decode durable catalog payloads and replay committed catalog mutation batches.
///
/// Decode failures, outer/inner kind mismatches, incomplete batches, and replay
/// anomalies are accumulated in the report. Only batches with a matching begin
/// and commit, dense non-duplicated apply indexes, matching catalog identity,
/// and contiguous catalog version are applied.
pub fn recover_catalog_snapshot_from_durable_payloads<'a>(
    snapshot: CatalogSnapshot,
    payloads: impl IntoIterator<Item = CatalogDurableMutationPayload<'a>>,
) -> CatalogRecoveryOutcome {
    let mut records = Vec::new();
    let mut anomalies = Vec::new();

    for (record_index, input) in payloads.into_iter().enumerate() {
        match CatalogMutationRecord::decode_durable_payload(input.payload) {
            Ok(record) => {
                if let Some(outer_tag) = input.storage_wal_kind_tag {
                    let inner_tag = record.kind().storage_wal_kind_tag();
                    if outer_tag != inner_tag {
                        anomalies.push(CatalogRecoveryAnomaly {
                            record_index,
                            batch_id: record.boundary_batch_id(),
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
            }
            Err(error) => {
                let detail = error.message().to_string();
                let kind = if detail.contains("kind tag") {
                    CatalogRecoveryAnomalyKind::WrongKindTag
                } else {
                    CatalogRecoveryAnomalyKind::PayloadCorruption
                };
                anomalies.push(CatalogRecoveryAnomaly {
                    record_index,
                    batch_id: None,
                    kind,
                    detail,
                });
            }
        }
    }

    replay_indexed_catalog_mutation_records(snapshot, records, anomalies)
}

/// Replay already decoded catalog mutation records.
///
/// This is useful for unit tests and for callers that have already validated the
/// durable payload envelope. Storage-WAL kind enforcement is only available via
/// [`recover_catalog_snapshot_from_durable_payloads`].
pub fn replay_catalog_mutation_records(
    snapshot: CatalogSnapshot,
    records: impl IntoIterator<Item = CatalogMutationRecord>,
) -> CatalogRecoveryOutcome {
    let indexed = records
        .into_iter()
        .enumerate()
        .map(|(record_index, record)| IndexedCatalogMutationRecord {
            record_index,
            record,
        })
        .collect();
    replay_indexed_catalog_mutation_records(snapshot, indexed, Vec::new())
}

fn replay_indexed_catalog_mutation_records(
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

                pending = Some(PendingCatalogBatch {
                    begin_record_index: indexed.record_index,
                    boundary,
                    deltas: Vec::new(),
                });
            }
            CatalogMutationRecord::Apply(delta) => {
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
            }
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
            }
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
    let mut plan = match CatalogMutationPlan::new(
        open.boundary.batch_id,
        open.boundary.database_id,
        open.boundary.namespace_id,
        open.boundary.previous_version,
        open.boundary.next_version,
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
        }
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
        applied_delta_count,
    });
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

trait CatalogMutationRecordRecoveryExt {
    fn boundary_batch_id(&self) -> Option<DefinitionBatchId>;
}

impl CatalogMutationRecordRecoveryExt for CatalogMutationRecord {
    fn boundary_batch_id(&self) -> Option<DefinitionBatchId> {
        match self {
            CatalogMutationRecord::Begin(boundary) | CatalogMutationRecord::Commit(boundary) => {
                Some(boundary.batch_id)
            }
            CatalogMutationRecord::Apply(_) => None,
        }
    }
}
