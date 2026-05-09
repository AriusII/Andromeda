use crate::CatalogSnapshot;

pub use andromeda_catalog_recovery::{
    CatalogDurableMutationPayload, CatalogRecoveredBatch, CatalogRecoveryAnomaly,
    CatalogRecoveryAnomalyKind, CatalogRecoveryReport, CatalogSkippedBatch,
    CatalogSkippedBatchReason,
};

/// Result of catalog recovery replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRecoveryOutcome {
    pub snapshot: CatalogSnapshot,
    pub report: CatalogRecoveryReport,
}
