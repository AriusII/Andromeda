use andromeda_core::{AndromedaResult, CatalogVersion, TransactionId};

use crate::wal_record_catalog::CatalogWalRecord;
use crate::{Lsn, WalRecord, WalRecordKind};

use super::codec::decode_catalog_record;
use super::error::{catalog_wal_error, lsn_order_error};
use super::transaction_filter::{
    catalog_record_transaction_id, require_apply_transaction_matches_begin,
    require_commit_transaction_matches_begin,
};

/// A single catalog mutation observed at a storage WAL LSN.
///
/// Storage keeps the original payload bytes for forensic replay evidence, but
/// `CatalogChangeApply` payloads must decode to a validated catalog WAL record
/// before they can participate in a durable publication report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogWalPublicationRecord {
    pub lsn: Lsn,
    pub payload: Vec<u8>,
    pub catalog_record: CatalogWalRecord,
}

/// A complete durable catalog publication reconstructed from storage WAL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogWalDurablePublication {
    pub transaction_id: TransactionId,
    pub begin_lsn: Lsn,
    pub commit_lsn: Lsn,
    pub begin_payload: Vec<u8>,
    pub apply_records: Vec<CatalogWalPublicationRecord>,
    pub commit_payload: Vec<u8>,
}

impl CatalogWalDurablePublication {
    pub fn apply_count(&self) -> usize {
        self.apply_records.len()
    }

    pub fn record_count(&self) -> usize {
        self.apply_records.len() + 2
    }

    pub fn decoded_catalog_records(&self) -> impl Iterator<Item = &CatalogWalRecord> {
        self.apply_records
            .iter()
            .map(|record| &record.catalog_record)
    }

    pub fn last_catalog_version(&self) -> Option<CatalogVersion> {
        self.apply_records
            .iter()
            .filter_map(|record| record.catalog_record.catalog_version())
            .next_back()
    }
}

/// Report produced by storage-side catalog publication replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogWalPublicationReplayReport {
    pub total_records: usize,
    pub catalog_records_seen: usize,
    pub incomplete_tail_records: usize,
    pub publications: Vec<CatalogWalDurablePublication>,
}

impl CatalogWalPublicationReplayReport {
    pub fn last_durable_publication(&self) -> Option<&CatalogWalDurablePublication> {
        self.publications.last()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingCatalogPublication {
    transaction_id: TransactionId,
    begin_lsn: Lsn,
    begin_payload: Vec<u8>,
    apply_records: Vec<CatalogWalPublicationRecord>,
}

struct CatalogPublicationReplayState {
    report: CatalogWalPublicationReplayReport,
    last_lsn: Option<Lsn>,
    pending: Option<PendingCatalogPublication>,
}

impl CatalogPublicationReplayState {
    fn new(total_records: usize) -> Self {
        Self {
            report: CatalogWalPublicationReplayReport {
                total_records,
                catalog_records_seen: 0,
                incomplete_tail_records: 0,
                publications: Vec::new(),
            },
            last_lsn: None,
            pending: None,
        }
    }

    fn observe_record(&mut self, record: &WalRecord) -> AndromedaResult<()> {
        self.validate_lsn_order(record.header.lsn)?;

        match record.header.kind {
            WalRecordKind::CatalogChangeBegin => self.observe_begin(record),
            WalRecordKind::CatalogChangeApply => self.observe_apply(record),
            WalRecordKind::CatalogChangeCommit => self.observe_commit(record),
            _ => Ok(()),
        }
    }

    fn into_report(mut self) -> CatalogWalPublicationReplayReport {
        if let Some(pending_publication) = self.pending.take() {
            self.report.incomplete_tail_records = pending_publication.apply_records.len() + 1;
        }
        self.report
    }

    fn validate_lsn_order(&mut self, lsn: Lsn) -> AndromedaResult<()> {
        if let Some(previous_lsn) = self.last_lsn
            && lsn <= previous_lsn
        {
            return Err(lsn_order_error(previous_lsn, lsn));
        }
        self.last_lsn = Some(lsn);
        Ok(())
    }

    fn observe_begin(&mut self, record: &WalRecord) -> AndromedaResult<()> {
        record.validate()?;
        self.report.catalog_records_seen += 1;
        if self.pending.is_some() {
            return Err(catalog_wal_error(
                "catalog WAL begin observed before the previous catalog publication committed",
            ));
        }

        self.pending = Some(PendingCatalogPublication {
            transaction_id: catalog_record_transaction_id(record)?,
            begin_lsn: record.header.lsn,
            begin_payload: record.payload().to_vec(),
            apply_records: Vec::new(),
        });
        Ok(())
    }

    fn observe_apply(&mut self, record: &WalRecord) -> AndromedaResult<()> {
        record.validate()?;
        self.report.catalog_records_seen += 1;
        let transaction_id = catalog_record_transaction_id(record)?;
        let Some(pending_publication) = self.pending.as_mut() else {
            return Err(catalog_wal_error(
                "catalog WAL apply observed before catalog publication begin",
            ));
        };
        require_apply_transaction_matches_begin(
            &pending_publication.transaction_id,
            &transaction_id,
        )?;

        pending_publication
            .apply_records
            .push(CatalogWalPublicationRecord {
                lsn: record.header.lsn,
                payload: record.payload().to_vec(),
                catalog_record: decode_apply_payload(record)?,
            });
        Ok(())
    }

    fn observe_commit(&mut self, record: &WalRecord) -> AndromedaResult<()> {
        record.validate()?;
        self.report.catalog_records_seen += 1;
        let transaction_id = catalog_record_transaction_id(record)?;
        let Some(pending_publication) = self.pending.take() else {
            return Err(catalog_wal_error(
                "catalog WAL commit observed before catalog publication begin",
            ));
        };
        require_commit_transaction_matches_begin(
            &pending_publication.transaction_id,
            &transaction_id,
        )?;
        if pending_publication.apply_records.is_empty() {
            return Err(catalog_wal_error(
                "catalog WAL publication commit requires at least one apply record",
            ));
        }

        self.report.publications.push(CatalogWalDurablePublication {
            transaction_id,
            begin_lsn: pending_publication.begin_lsn,
            commit_lsn: record.header.lsn,
            begin_payload: pending_publication.begin_payload,
            apply_records: pending_publication.apply_records,
            commit_payload: record.payload().to_vec(),
        });
        Ok(())
    }
}

fn decode_apply_payload(record: &WalRecord) -> AndromedaResult<CatalogWalRecord> {
    decode_catalog_record(record.payload()).map_err(|err| {
        catalog_wal_error(format!(
            "catalog WAL apply payload at LSN {} is not a valid catalog record: {}",
            record.header.lsn.get(),
            err.message()
        ))
    })
}

/// Replays catalog publication boundaries from storage WAL records.
///
/// Only complete `CatalogChangeBegin` -> one or more `CatalogChangeApply` ->
/// `CatalogChangeCommit` spans become durable publications. An incomplete tail
/// is reported but not published, preserving WAL-before-visible-commit.
/// `CatalogChangeApply` payloads are decoded and validated into catalog WAL
/// records before publication evidence is retained; begin and commit payloads
/// remain boundary metadata owned by the storage WAL bridge.
pub fn replay_catalog_publications_from_wal(
    records: &[WalRecord],
) -> AndromedaResult<CatalogWalPublicationReplayReport> {
    let mut state = CatalogPublicationReplayState::new(records.len());

    for record in records {
        state.observe_record(record)?;
    }

    Ok(state.into_report())
}
