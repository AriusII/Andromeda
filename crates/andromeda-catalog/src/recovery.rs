//! Catalog mutation recovery replay.
//!
//! Recovery consumes the durable catalog mutation payloads produced by
//! [`crate::CatalogMutationRecord::encode_durable_payload`], reconstructs only
//! fully committed begin/apply/commit batches, and replays those batches into a
//! [`crate::CatalogSnapshot`].  Incomplete or anomalous batches are not applied.

mod replay;
mod types;

pub use types::*;

use crate::{
    CatalogMutationRecord, CatalogSnapshot,
    recovery::replay::{indexed_recovery_record, replay_indexed_catalog_mutation_records},
};

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
    let indexed = records
        .into_iter()
        .enumerate()
        .map(|(record_index, record)| indexed_recovery_record(record_index, record))
        .collect();
    replay_indexed_catalog_mutation_records(snapshot, indexed, Vec::new())
}
