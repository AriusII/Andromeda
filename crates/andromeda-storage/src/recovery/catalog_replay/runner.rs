use andromeda_core::{AndromedaResult, CatalogVersion};

use crate::Lsn;
use crate::wal_record_catalog::CatalogWalRecord;

use super::selection::CatalogReplaySelection;
use super::state::CatalogReplayState;
use super::{CatalogReplayFromLsnReport, CatalogSnapshot, LsnBoundCatalogRecord};

/// Replays a segment of catalog WAL records to reconstruct the catalog at a target version.
///
/// # Preconditions
///
/// - `records` must be in LSN order (monotonically increasing).
/// - All records must have version <= `target_catalog_version`.
/// - Records must be valid (already validated during encoding).
///
/// # Postconditions
///
/// - Returned snapshot has `catalog_version` exactly matching the last replayed catalog version.
/// - All procedure IDs in `procedure_ids` correspond to procedures visible at the final version.
/// - Version monotonicity is guaranteed (no version reordering).
/// - Procedure existence is validated (all references exist).
///
/// # Error Conditions
///
/// - Version reordering (version goes backward)
/// - Procedure reference to non-existent ID
/// - Invalid record invariants (caught during validation)
pub fn replay_catalog_wal_records(
    records: &[CatalogWalRecord],
    target_catalog_version: CatalogVersion,
) -> AndromedaResult<CatalogSnapshot> {
    let mut state = CatalogReplayState::new();

    for record in records {
        if state.record_is_beyond_target(record, target_catalog_version) {
            continue;
        }

        state.validate_and_advance_version(record)?;
        state.replay_record(record)?;
    }

    Ok(state.into_snapshot())
}

/// Replay catalog WAL records starting from a manifest-required LSN.
///
/// This function extends [`replay_catalog_wal_records`] with LSN-based
/// filtering so that only records at or after `catalog_wal_start_lsn`
/// participate in replay.
///
/// # Recovery Protocol
///
/// 1. Filter `lsn_records` to those with `storage_lsn >= catalog_wal_start_lsn`.
/// 2. Extract the `CatalogWalRecord` slice from the eligible entries (order preserved).
/// 3. Call [`replay_catalog_wal_records`] on the filtered slice.
/// 4. Return [`CatalogReplayFromLsnReport`] with replay statistics.
///
/// # Preconditions
///
/// * `lsn_records` must be in ascending `storage_lsn` order.
/// * `catalog_wal_start_lsn` must match the manifest's
///   `CatalogWalStartLsn` (or equivalent anchor).
///
/// # Errors
///
/// Propagates any error from [`replay_catalog_wal_records`]:
/// - Catalog version reordering.
/// - Procedure reference to non-existent ID.
/// - `CatalogCheckpoint` visible-count mismatch.
pub fn replay_catalog_from_lsn(
    lsn_records: &[LsnBoundCatalogRecord],
    catalog_wal_start_lsn: Lsn,
    target_catalog_version: CatalogVersion,
) -> AndromedaResult<CatalogReplayFromLsnReport> {
    let selection = CatalogReplaySelection::select(lsn_records, catalog_wal_start_lsn)?;
    let snapshot =
        replay_catalog_wal_records(selection.eligible_records(), target_catalog_version)?;

    Ok(CatalogReplayFromLsnReport {
        catalog_wal_start_lsn,
        end_lsn: selection.end_lsn(),
        total_records: lsn_records.len(),
        records_below_floor: selection.records_below_floor(),
        records_replayed: selection.records_replayed(),
        snapshot,
    })
}
