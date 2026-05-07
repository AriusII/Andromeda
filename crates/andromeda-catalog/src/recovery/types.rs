use andromeda_types::CatalogVersion;

use crate::{CatalogSnapshot, DefinitionBatchId};

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
    pub source_hash: crate::DefinitionBatchSourceHash,
    pub dependency_graph_hash: crate::DefinitionBatchDependencyGraphHash,
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
    ApplyRecordCountMismatch,
    ApplyRecordLimitExceeded,
    ApplyRecordOrderMismatch,
    WrongCatalogIdentity,
    VersionGap,
    DefinitionBatchHashMismatch,
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
    PayloadMagicMismatch,
    PayloadFormatVersionMismatch,
    PayloadChecksumMismatch,
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
    ApplyRecordCountMismatch,
    ApplyRecordLimitExceeded,
    ApplyRecordOrderMismatch,
    PlanRejected,
    DefinitionBatchHashMismatch,
    ReplayRejected,
}
