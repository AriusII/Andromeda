use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_wal::Lsn;

/// A catalog recovery record paired with the outer storage-WAL LSN that
/// carried it.
///
/// The record type is generic so storage can keep its legacy semantic record
/// adapter while catalog-recovery owns the LSN filtering DTO boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LsnBoundCatalogRecord<Record> {
    /// LSN of the outer storage WAL record that carried this catalog record.
    pub storage_lsn: Lsn,
    /// The catalog recovery record itself.
    pub record: Record,
}

impl<Record> LsnBoundCatalogRecord<Record> {
    pub fn new(storage_lsn: Lsn, record: Record) -> Self {
        Self {
            storage_lsn,
            record,
        }
    }
}

/// Report from an LSN-anchored catalog replay session.
///
/// Snapshot ownership stays with the catalog/storage adapter that supplies the
/// concrete `Snapshot` type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogReplayFromLsnReport<Snapshot> {
    /// The catalog WAL start LSN used as the replay floor.
    pub catalog_wal_start_lsn: Lsn,
    /// Highest LSN of the replayed records, or `None` if nothing was replayed.
    pub end_lsn: Option<Lsn>,
    /// Total records provided before floor filtering.
    pub total_records: usize,
    /// Records whose storage LSN was below the replay floor.
    pub records_below_floor: usize,
    /// Records whose storage LSN was at or above the replay floor.
    pub records_replayed: usize,
    /// Adapter-owned catalog snapshot/projection after replay.
    pub snapshot: Snapshot,
}

impl<Snapshot> CatalogReplayFromLsnReport<Snapshot> {
    /// Returns `true` when all provided records were replayed.
    pub fn all_records_replayed(&self) -> bool {
        self.records_below_floor == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogLsnReplaySelection<Record> {
    records_below_floor: usize,
    eligible_records: Vec<Record>,
    end_lsn: Option<Lsn>,
}

impl<Record: Clone> CatalogLsnReplaySelection<Record> {
    pub fn select(
        lsn_records: &[LsnBoundCatalogRecord<Record>],
        catalog_wal_start_lsn: Lsn,
    ) -> AndromedaResult<Self> {
        let mut previous_lsn = None;
        let mut records_below_floor = 0;
        let mut eligible_records = Vec::new();
        let mut end_lsn = None;

        for record in lsn_records {
            if let Some(previous) = previous_lsn
                && record.storage_lsn < previous
            {
                return Err(catalog_recovery_error(format!(
                    "catalog WAL replay records must be in ascending storage LSN order: previous={}, current={}",
                    previous.get(),
                    record.storage_lsn.get()
                )));
            }
            previous_lsn = Some(record.storage_lsn);

            if record.storage_lsn < catalog_wal_start_lsn {
                records_below_floor += 1;
                continue;
            }

            end_lsn = Some(record.storage_lsn);
            eligible_records.push(record.record.clone());
        }

        Ok(Self {
            records_below_floor,
            eligible_records,
            end_lsn,
        })
    }

    pub const fn records_below_floor(&self) -> usize {
        self.records_below_floor
    }

    pub fn records_replayed(&self) -> usize {
        self.eligible_records.len()
    }

    pub const fn end_lsn(&self) -> Option<Lsn> {
        self.end_lsn
    }

    pub fn eligible_records(&self) -> &[Record] {
        &self.eligible_records
    }
}

fn catalog_recovery_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Catalog, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsn_selection_filters_records_below_floor() {
        let records = vec![
            LsnBoundCatalogRecord::new(Lsn::new(5), "below"),
            LsnBoundCatalogRecord::new(Lsn::new(10), "at_floor"),
            LsnBoundCatalogRecord::new(Lsn::new(12), "after_floor"),
        ];

        let selection = CatalogLsnReplaySelection::select(&records, Lsn::new(10)).unwrap();

        assert_eq!(selection.records_below_floor(), 1);
        assert_eq!(selection.records_replayed(), 2);
        assert_eq!(selection.end_lsn(), Some(Lsn::new(12)));
        assert_eq!(selection.eligible_records(), &["at_floor", "after_floor"]);
    }

    #[test]
    fn lsn_selection_rejects_reordered_storage_lsn() {
        let records = vec![
            LsnBoundCatalogRecord::new(Lsn::new(9), "first"),
            LsnBoundCatalogRecord::new(Lsn::new(8), "second"),
        ];

        let err = CatalogLsnReplaySelection::select(&records, Lsn::ZERO).unwrap_err();

        assert_eq!(err.kind(), AndromedaErrorKind::Catalog);
        assert!(err.message().contains("ascending storage LSN order"));
    }
}
