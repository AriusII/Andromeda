//! Catalog mutation recovery replay.
//!
//! Recovery consumes the durable catalog mutation payloads produced by
//! [`crate::CatalogMutationRecord::encode_durable_payload`], reconstructs only
//! fully committed begin/apply/commit batches, and replays those batches into a
//! [`crate::CatalogSnapshot`].  Incomplete or anomalous batches are not applied.

mod decode;
mod replay;
mod types;

pub use types::*;

use crate::{
    CatalogMutationRecord, CatalogSnapshot,
    recovery::{
        decode::boundary_batch_id,
        replay::{IndexedCatalogMutationRecord, replay_indexed_catalog_mutation_records},
    },
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
    let mut records = Vec::new();
    let mut anomalies = Vec::new();

    for (record_index, input) in payloads.into_iter().enumerate() {
        match CatalogMutationRecord::decode_durable_payload_typed(input.payload) {
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
                    kind: andromeda_catalog_recovery::recovery_anomaly_kind_for_decode_error(
                        error.kind(),
                    ),
                    detail: format!("{}: {}", error.kind().stable_code(), error.detail()),
                });
            },
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
