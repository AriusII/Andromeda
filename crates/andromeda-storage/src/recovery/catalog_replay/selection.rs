use andromeda_core::AndromedaResult;

use crate::Lsn;
use crate::wal_record_catalog::CatalogWalRecord;

use super::super::storage_error;
use super::LsnBoundCatalogRecord;

pub(super) struct CatalogReplaySelection {
    records_below_floor: usize,
    eligible_records: Vec<CatalogWalRecord>,
    end_lsn: Option<Lsn>,
}

impl CatalogReplaySelection {
    pub(super) fn select(
        lsn_records: &[LsnBoundCatalogRecord],
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
                return Err(storage_error(format!(
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

    pub(super) const fn records_below_floor(&self) -> usize {
        self.records_below_floor
    }

    pub(super) const fn records_replayed(&self) -> usize {
        self.eligible_records.len()
    }

    pub(super) const fn end_lsn(&self) -> Option<Lsn> {
        self.end_lsn
    }

    pub(super) fn eligible_records(&self) -> &[CatalogWalRecord] {
        &self.eligible_records
    }
}
