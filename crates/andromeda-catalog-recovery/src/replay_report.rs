//! Runtime-free catalog recovery replay report DTOs.

use andromeda_definition_batch::{
    DefinitionBatchDependencyGraphHash, DefinitionBatchId, DefinitionBatchSourceHash,
};
use andromeda_types::CatalogVersion;

use crate::{
    CatalogRecoveryAnomalyKind, CatalogSkippedBatchReason, CatalogWalPayloadDecodeErrorKind,
};

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
    pub source_hash: DefinitionBatchSourceHash,
    pub dependency_graph_hash: DefinitionBatchDependencyGraphHash,
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

pub fn recovery_anomaly_kind_for_decode_error(
    kind: CatalogWalPayloadDecodeErrorKind,
) -> CatalogRecoveryAnomalyKind {
    match kind {
        CatalogWalPayloadDecodeErrorKind::MagicMismatch => {
            CatalogRecoveryAnomalyKind::PayloadMagicMismatch
        },
        CatalogWalPayloadDecodeErrorKind::LegacyFormatVersion
        | CatalogWalPayloadDecodeErrorKind::UnsupportedFormatVersion => {
            CatalogRecoveryAnomalyKind::PayloadFormatVersionMismatch
        },
        CatalogWalPayloadDecodeErrorKind::ChecksumMismatch => {
            CatalogRecoveryAnomalyKind::PayloadChecksumMismatch
        },
        CatalogWalPayloadDecodeErrorKind::UnknownRecordKindTag => {
            CatalogRecoveryAnomalyKind::WrongKindTag
        },
        CatalogWalPayloadDecodeErrorKind::TruncatedHeader
        | CatalogWalPayloadDecodeErrorKind::BodyLengthOverflow
        | CatalogWalPayloadDecodeErrorKind::BodyLengthMismatch
        | CatalogWalPayloadDecodeErrorKind::BodyInvalid => {
            CatalogRecoveryAnomalyKind::PayloadCorruption
        },
    }
}
