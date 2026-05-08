use andromeda_types::CatalogVersion;

use crate::{CatalogSnapshot, DefinitionBatchId};

pub use andromeda_catalog_recovery::{
    CatalogDurableMutationPayload, CatalogRecoveryAnomalyKind, CatalogSkippedBatchReason,
};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRecoveryAnomaly {
    pub record_index: usize,
    pub batch_id: Option<DefinitionBatchId>,
    pub kind: CatalogRecoveryAnomalyKind,
    pub detail: String,
}
