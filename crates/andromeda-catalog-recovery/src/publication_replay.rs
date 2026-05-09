//! Runtime-free catalog publication replay contracts for storage WAL boundaries.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, TransactionId};
use andromeda_wal::Lsn;

use crate::CatalogMutationRecordKind;

/// Catalog metadata needed by durable publication reports.
pub trait CatalogWalPublicationRecordMetadata {
    fn catalog_version(&self) -> Option<CatalogVersion>;
}

impl CatalogWalPublicationRecordMetadata for crate::CatalogWalRecord {
    fn catalog_version(&self) -> Option<CatalogVersion> {
        self.catalog_version()
    }
}

/// Storage-WAL catalog record projected into recovery-owned replay inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogWalObservedRecord<'a> {
    pub lsn: Lsn,
    pub kind: CatalogMutationRecordKind,
    pub transaction_id: Option<TransactionId>,
    pub payload: &'a [u8],
}

/// A single catalog mutation observed at a storage WAL LSN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogWalPublicationRecord<CatalogRecord> {
    pub lsn: Lsn,
    pub payload: Vec<u8>,
    pub catalog_record: CatalogRecord,
}

/// A complete durable catalog publication reconstructed from storage WAL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogWalDurablePublication<CatalogRecord> {
    pub transaction_id: TransactionId,
    pub begin_lsn: Lsn,
    pub commit_lsn: Lsn,
    pub begin_payload: Vec<u8>,
    pub apply_records: Vec<CatalogWalPublicationRecord<CatalogRecord>>,
    pub commit_payload: Vec<u8>,
}

impl<CatalogRecord> CatalogWalDurablePublication<CatalogRecord> {
    pub fn apply_count(&self) -> usize {
        self.apply_records.len()
    }

    pub fn record_count(&self) -> usize {
        self.apply_records.len() + 2
    }

    pub fn decoded_catalog_records(&self) -> impl Iterator<Item = &CatalogRecord> {
        self.apply_records
            .iter()
            .map(|record| &record.catalog_record)
    }
}

impl<CatalogRecord> CatalogWalDurablePublication<CatalogRecord>
where
    CatalogRecord: CatalogWalPublicationRecordMetadata,
{
    pub fn last_catalog_version(&self) -> Option<CatalogVersion> {
        self.apply_records
            .iter()
            .filter_map(|record| record.catalog_record.catalog_version())
            .next_back()
    }
}

/// Report produced by catalog publication replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogWalPublicationReplayReport<CatalogRecord> {
    pub total_records: usize,
    pub catalog_records_seen: usize,
    pub incomplete_tail_records: usize,
    pub publications: Vec<CatalogWalDurablePublication<CatalogRecord>>,
}

impl<CatalogRecord> CatalogWalPublicationReplayReport<CatalogRecord> {
    pub fn last_durable_publication(&self) -> Option<&CatalogWalDurablePublication<CatalogRecord>> {
        self.publications.last()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingCatalogPublication<CatalogRecord> {
    transaction_id: TransactionId,
    begin_lsn: Lsn,
    begin_payload: Vec<u8>,
    apply_records: Vec<CatalogWalPublicationRecord<CatalogRecord>>,
}

struct CatalogPublicationReplayState<CatalogRecord> {
    report: CatalogWalPublicationReplayReport<CatalogRecord>,
    last_lsn: Option<Lsn>,
    pending: Option<PendingCatalogPublication<CatalogRecord>>,
}

impl<CatalogRecord> CatalogPublicationReplayState<CatalogRecord> {
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

    fn observe_record<F>(
        &mut self,
        record: CatalogWalObservedRecord<'_>,
        decode_apply_payload: &mut F,
    ) -> AndromedaResult<()>
    where
        F: FnMut(CatalogWalObservedRecord<'_>) -> AndromedaResult<CatalogRecord>,
    {
        self.validate_lsn_order(record.lsn)?;

        match record.kind {
            CatalogMutationRecordKind::CatalogChangeBegin => self.observe_begin(record),
            CatalogMutationRecordKind::CatalogChangeApply => {
                self.observe_apply(record, decode_apply_payload)
            },
            CatalogMutationRecordKind::CatalogChangeCommit => self.observe_commit(record),
        }
    }

    fn into_report(mut self) -> CatalogWalPublicationReplayReport<CatalogRecord> {
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

    fn observe_begin(&mut self, record: CatalogWalObservedRecord<'_>) -> AndromedaResult<()> {
        self.report.catalog_records_seen += 1;
        if self.pending.is_some() {
            return Err(catalog_wal_error(
                "catalog WAL begin observed before the previous catalog publication committed",
            ));
        }

        self.pending = Some(PendingCatalogPublication {
            transaction_id: catalog_record_transaction_id(record)?,
            begin_lsn: record.lsn,
            begin_payload: record.payload.to_vec(),
            apply_records: Vec::new(),
        });
        Ok(())
    }

    fn observe_apply<F>(
        &mut self,
        record: CatalogWalObservedRecord<'_>,
        decode_apply_payload: &mut F,
    ) -> AndromedaResult<()>
    where
        F: FnMut(CatalogWalObservedRecord<'_>) -> AndromedaResult<CatalogRecord>,
    {
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
                lsn: record.lsn,
                payload: record.payload.to_vec(),
                catalog_record: decode_apply_payload(record)?,
            });
        Ok(())
    }

    fn observe_commit(&mut self, record: CatalogWalObservedRecord<'_>) -> AndromedaResult<()> {
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
            commit_lsn: record.lsn,
            begin_payload: pending_publication.begin_payload,
            apply_records: pending_publication.apply_records,
            commit_payload: record.payload.to_vec(),
        });
        Ok(())
    }
}

pub fn replay_catalog_publications_from_observed_wal<'a, I, CatalogRecord, F>(
    total_records: usize,
    records: I,
    mut decode_apply_payload: F,
) -> AndromedaResult<CatalogWalPublicationReplayReport<CatalogRecord>>
where
    I: IntoIterator<Item = CatalogWalObservedRecord<'a>>,
    F: FnMut(CatalogWalObservedRecord<'_>) -> AndromedaResult<CatalogRecord>,
{
    let mut state = CatalogPublicationReplayState::new(total_records);

    for record in records {
        state.observe_record(record, &mut decode_apply_payload)?;
    }

    Ok(state.into_report())
}

fn catalog_record_transaction_id(
    record: CatalogWalObservedRecord<'_>,
) -> AndromedaResult<TransactionId> {
    record.transaction_id.ok_or_else(|| {
        catalog_wal_error("catalog WAL publication record requires a transaction id")
    })
}

fn require_apply_transaction_matches_begin(
    begin_transaction_id: &TransactionId,
    apply_transaction_id: &TransactionId,
) -> AndromedaResult<()> {
    if begin_transaction_id != apply_transaction_id {
        return Err(catalog_wal_error(
            "catalog WAL apply transaction id does not match publication begin",
        ));
    }

    Ok(())
}

fn require_commit_transaction_matches_begin(
    begin_transaction_id: &TransactionId,
    commit_transaction_id: &TransactionId,
) -> AndromedaResult<()> {
    if begin_transaction_id != commit_transaction_id {
        return Err(catalog_wal_error(
            "catalog WAL commit transaction id does not match publication begin",
        ));
    }

    Ok(())
}

fn catalog_wal_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message.into())
}

fn lsn_order_error(previous: Lsn, current: Lsn) -> AndromedaError {
    catalog_wal_error(format!(
        "catalog WAL replay requires strictly increasing LSN order: previous {}, current {}",
        previous.get(),
        current.get()
    ))
}
