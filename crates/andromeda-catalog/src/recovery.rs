//! Compatibility facade for catalog snapshot recovery.

use crate::{CatalogMutationRecord, CatalogSnapshot};

pub use andromeda_catalog_recovery::{
    CatalogDurableMutationPayload, CatalogRecoveredBatch, CatalogRecoveryAnomaly,
    CatalogRecoveryAnomalyKind, CatalogRecoveryReport, CatalogSkippedBatch,
    CatalogSkippedBatchReason,
};

/// Result of catalog snapshot recovery replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRecoveryOutcome {
    pub snapshot: CatalogSnapshot,
    pub report: CatalogRecoveryReport,
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
    let outcome = andromeda_catalog_recovery::recover_catalog_target_from_durable_payloads(
        snapshot, payloads,
    );
    CatalogRecoveryOutcome {
        snapshot: outcome.target,
        report: outcome.report,
    }
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
    let outcome = andromeda_catalog_recovery::replay_catalog_mutation_records_into_target(
        snapshot,
        records
            .into_iter()
            .map(andromeda_catalog_recovery::CatalogMutationRecord::from),
    );
    CatalogRecoveryOutcome {
        snapshot: outcome.target,
        report: outcome.report,
    }
}
