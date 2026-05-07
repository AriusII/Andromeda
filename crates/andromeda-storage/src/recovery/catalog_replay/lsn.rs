use crate::Lsn;
use crate::wal_record_catalog::CatalogWalRecord;

use super::snapshot::CatalogSnapshot;

/// A catalog WAL record paired with its storage-WAL LSN for LSN-based
/// filtering during crash recovery.
///
/// The storage WAL LSN is the LSN of the outer storage WAL record that
/// carried this catalog WAL record as its payload
/// (e.g. `CatalogChangeApply` or `CatalogChangeCommit`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LsnBoundCatalogRecord {
    /// LSN of the outer storage WAL record that carried this catalog record.
    pub storage_lsn: Lsn,
    /// The catalog WAL record itself.
    pub record: CatalogWalRecord,
}

impl LsnBoundCatalogRecord {
    pub fn new(storage_lsn: Lsn, record: CatalogWalRecord) -> Self {
        Self {
            storage_lsn,
            record,
        }
    }
}

/// Report from an LSN-anchored catalog replay session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogReplayFromLsnReport {
    /// The catalog WAL start LSN used as the replay floor.
    pub catalog_wal_start_lsn: Lsn,
    /// Highest LSN of the replayed records, or `None` if nothing was replayed.
    pub end_lsn: Option<Lsn>,
    /// Total records provided (before floor filtering).
    pub total_records: usize,
    /// Records whose storage LSN was below the replay floor (filtered out).
    pub records_below_floor: usize,
    /// Records whose storage LSN was at or above the replay floor (replayed).
    pub records_replayed: usize,
    /// Catalog snapshot after replay.
    pub snapshot: CatalogSnapshot,
}

impl CatalogReplayFromLsnReport {
    /// Returns `true` when all provided records were replayed (none filtered).
    pub fn all_records_replayed(&self) -> bool {
        self.records_below_floor == 0
    }
}
