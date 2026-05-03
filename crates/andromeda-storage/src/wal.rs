use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use std::collections::BTreeMap;

use crate::Lsn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalRecordKind {
    TxBegin,
    TxCommit,
    TxRollback,
    PageAllocate,
    PageFormat,
    RowInsert,
    RowUpdate,
    RowDelete,
    IndexInsert,
    IndexDelete,
    MvccVersionCreate,
    MvccVersionClose,
    MapDeltaAppend,
    CheckpointBegin,
    CheckpointEnd,
    SnapshotBegin,
    SnapshotEnd,
    ManifestSwitch,
    CatalogChangeBegin,
    CatalogChangeApply,
    CatalogChangeCommit,
    SecurityAuditAppend,
}

impl WalRecordKind {
    pub const fn is_transaction_boundary(self) -> bool {
        matches!(self, Self::TxBegin | Self::TxCommit | Self::TxRollback)
    }

    pub const fn requires_transaction_id(self) -> bool {
        matches!(
            self,
            Self::TxBegin
                | Self::TxCommit
                | Self::TxRollback
                | Self::RowInsert
                | Self::RowUpdate
                | Self::RowDelete
                | Self::IndexInsert
                | Self::IndexDelete
                | Self::MvccVersionCreate
                | Self::MvccVersionClose
                | Self::CatalogChangeBegin
                | Self::CatalogChangeApply
                | Self::CatalogChangeCommit
        )
    }

    pub const fn is_redo_relevant(self) -> bool {
        matches!(
            self,
            Self::PageAllocate
                | Self::PageFormat
                | Self::RowInsert
                | Self::RowUpdate
                | Self::RowDelete
                | Self::IndexInsert
                | Self::IndexDelete
                | Self::MvccVersionCreate
                | Self::MvccVersionClose
                | Self::MapDeltaAppend
                | Self::ManifestSwitch
                | Self::CatalogChangeApply
                | Self::CatalogChangeCommit
                | Self::SecurityAuditAppend
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalRecordHeader {
    pub kind: WalRecordKind,
    pub lsn: Lsn,
    pub previous_lsn: Option<Lsn>,
    pub transaction_id: Option<TransactionId>,
    pub payload_length: u64,
    pub checksum: u64,
}

impl WalRecordHeader {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.lsn == Lsn::default() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL record LSN must not be zero",
            ));
        }

        if self.kind.requires_transaction_id() && self.transaction_id.is_none() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL record kind requires a transaction id",
            ));
        }

        if self.checksum == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL record checksum must not be zero",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalRecord {
    pub header: WalRecordHeader,
    pub payload: Vec<u8>,
}

impl WalRecord {
    pub fn new(header: WalRecordHeader, payload: Vec<u8>) -> AndromedaResult<Self> {
        let record = Self { header, payload };
        record.validate()?;
        Ok(record)
    }

    pub fn from_parts(
        kind: WalRecordKind,
        lsn: Lsn,
        previous_lsn: Option<Lsn>,
        transaction_id: Option<TransactionId>,
        payload: impl Into<Vec<u8>>,
    ) -> AndromedaResult<Self> {
        let payload = payload.into();
        let payload_length = payload.len() as u64;
        let checksum = wal_record_checksum(kind, lsn, previous_lsn, transaction_id, &payload);
        Self::new(
            WalRecordHeader {
                kind,
                lsn,
                previous_lsn,
                transaction_id,
                payload_length,
                checksum,
            },
            payload,
        )
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.header.validate()?;

        if self.header.payload_length != self.payload.len() as u64 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL record payload length mismatch",
            ));
        }

        if matches!(self.header.previous_lsn, Some(previous_lsn) if previous_lsn >= self.header.lsn)
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL record previous LSN must precede record LSN",
            ));
        }

        let expected_checksum = wal_record_checksum(
            self.header.kind,
            self.header.lsn,
            self.header.previous_lsn,
            self.header.transaction_id,
            &self.payload,
        );
        if self.header.checksum != expected_checksum {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL record checksum mismatch",
            ));
        }

        Ok(())
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub const fn transaction_id(&self) -> Option<TransactionId> {
        self.header.transaction_id
    }

    pub fn is_for_transaction(&self, transaction_id: TransactionId) -> bool {
        matches!(self.header.transaction_id, Some(id) if id.get() == transaction_id.get())
    }

    pub const fn is_transaction_terminal(&self) -> bool {
        matches!(
            self.header.kind,
            WalRecordKind::TxCommit | WalRecordKind::TxRollback
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurableTransactionState {
    Open,
    Committed,
    RolledBack,
    Incomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableTransactionResume {
    pub transaction_id: TransactionId,
    pub state: DurableTransactionState,
    pub first_lsn: Lsn,
    pub last_lsn: Lsn,
    pub begin_lsn: Option<Lsn>,
    pub commit_lsn: Option<Lsn>,
    pub rollback_lsn: Option<Lsn>,
    pub record_count: usize,
}

impl DurableTransactionResume {
    pub const fn is_complete(self) -> bool {
        matches!(
            self.state,
            DurableTransactionState::Committed | DurableTransactionState::RolledBack
        )
    }

    pub const fn is_incomplete(self) -> bool {
        matches!(
            self.state,
            DurableTransactionState::Open | DurableTransactionState::Incomplete
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncompleteDurableTransaction {
    pub transaction_id: TransactionId,
    pub first_lsn: Lsn,
    pub last_lsn: Lsn,
    pub record_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DurableTransactionClassifications {
    pub committed: Vec<DurableTransactionResume>,
    pub rolled_back: Vec<DurableTransactionResume>,
    pub incomplete: Vec<DurableTransactionResume>,
}

impl DurableTransactionClassifications {
    pub fn is_empty(&self) -> bool {
        self.committed.is_empty() && self.rolled_back.is_empty() && self.incomplete.is_empty()
    }

    pub fn committed_transaction_ids(&self) -> impl Iterator<Item = TransactionId> + '_ {
        self.committed.iter().map(|summary| summary.transaction_id)
    }

    pub fn rolled_back_transaction_ids(&self) -> impl Iterator<Item = TransactionId> + '_ {
        self.rolled_back
            .iter()
            .map(|summary| summary.transaction_id)
    }

    pub fn incomplete_transaction_ids(&self) -> impl Iterator<Item = TransactionId> + '_ {
        self.incomplete.iter().map(|summary| summary.transaction_id)
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryWal {
    records: Vec<WalRecord>,
    durable_lsn: Lsn,
}

impl InMemoryWal {
    pub fn new() -> Self {
        Self::default()
    }

    pub const fn durable_lsn(&self) -> Lsn {
        self.durable_lsn
    }

    pub fn last_lsn(&self) -> Option<Lsn> {
        self.records.last().map(|record| record.header.lsn)
    }

    pub fn next_lsn(&self) -> Lsn {
        self.try_next_lsn().unwrap_or(Lsn::ZERO)
    }

    pub fn try_next_lsn(&self) -> AndromedaResult<Lsn> {
        match self.last_lsn() {
            Some(last_lsn) => last_lsn.try_next(),
            None => Ok(Lsn::new(1)),
        }
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn records(&self) -> &[WalRecord] {
        &self.records
    }

    pub fn append(&mut self, record: WalRecord) -> AndromedaResult<Lsn> {
        record.validate()?;

        let expected_lsn = self.try_next_lsn()?;
        if record.header.lsn != expected_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL record LSN must equal the next append LSN",
            ));
        }

        let lsn = record.header.lsn;
        self.records.push(record);
        Ok(lsn)
    }

    pub fn append_payload(
        &mut self,
        kind: WalRecordKind,
        transaction_id: Option<TransactionId>,
        payload: impl Into<Vec<u8>>,
    ) -> AndromedaResult<Lsn> {
        let record = WalRecord::from_parts(
            kind,
            self.try_next_lsn()?,
            self.last_lsn(),
            transaction_id,
            payload,
        )?;
        self.append(record)
    }

    pub fn append_tx_begin(&mut self, transaction_id: TransactionId) -> AndromedaResult<Lsn> {
        self.append_payload(WalRecordKind::TxBegin, Some(transaction_id), Vec::new())
    }

    pub fn append_tx_commit(&mut self, transaction_id: TransactionId) -> AndromedaResult<Lsn> {
        self.append_payload(WalRecordKind::TxCommit, Some(transaction_id), Vec::new())
    }

    pub fn append_tx_rollback(&mut self, transaction_id: TransactionId) -> AndromedaResult<Lsn> {
        self.append_payload(WalRecordKind::TxRollback, Some(transaction_id), Vec::new())
    }

    pub fn flush_through(&mut self, lsn: Lsn) -> AndromedaResult<Lsn> {
        if lsn == Lsn::default() {
            return Ok(self.durable_lsn);
        }

        let Some(last_lsn) = self.last_lsn() else {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "cannot flush WAL before records are appended",
            ));
        };

        if lsn > last_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "cannot flush WAL beyond the last appended LSN",
            ));
        }

        if lsn > self.durable_lsn {
            self.durable_lsn = lsn;
        }

        Ok(self.durable_lsn)
    }

    pub fn flush_all(&mut self) -> AndromedaResult<Lsn> {
        match self.last_lsn() {
            Some(last_lsn) => self.flush_through(last_lsn),
            None => Ok(self.durable_lsn),
        }
    }

    pub fn durable_records(&self) -> impl Iterator<Item = &WalRecord> {
        let durable_lsn = self.durable_lsn;
        self.records
            .iter()
            .filter(move |record| record.header.lsn <= durable_lsn)
    }

    pub fn replay_durable(&self) -> Vec<WalRecord> {
        self.durable_records().cloned().collect()
    }

    pub fn durable_records_for_transaction(
        &self,
        transaction_id: TransactionId,
    ) -> Vec<&WalRecord> {
        self.durable_records()
            .filter(|record| record.is_for_transaction(transaction_id))
            .collect()
    }

    pub fn resume_transaction(
        &self,
        transaction_id: TransactionId,
    ) -> Option<DurableTransactionResume> {
        summarize_transaction(
            transaction_id,
            self.durable_records_for_transaction(transaction_id),
        )
    }

    pub fn incomplete_durable_transactions(&self) -> Vec<IncompleteDurableTransaction> {
        incomplete_transactions_from_records(self.durable_records())
    }

    pub fn classify_durable_transactions(&self) -> DurableTransactionClassifications {
        classify_durable_transactions(self.durable_records())
    }
}

pub type MemoryWal = InMemoryWal;

pub(crate) fn wal_record_checksum(
    kind: WalRecordKind,
    lsn: Lsn,
    previous_lsn: Option<Lsn>,
    transaction_id: Option<TransactionId>,
    payload: &[u8],
) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    fn fold_u64(state: &mut u64, value: u64) {
        for byte in value.to_le_bytes() {
            *state ^= u64::from(byte);
            *state = state.wrapping_mul(FNV_PRIME);
        }
    }

    let mut state = FNV_OFFSET;
    fold_u64(&mut state, wal_record_kind_tag(kind));
    fold_u64(&mut state, lsn.get());
    fold_u64(&mut state, previous_lsn.map_or(0, Lsn::get));
    fold_u64(&mut state, transaction_id.map_or(0, TransactionId::get));
    fold_u64(&mut state, payload.len() as u64);

    for byte in payload {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(FNV_PRIME);
    }

    if state == 0 { 1 } else { state }
}

pub(crate) fn wal_record_kind_tag(kind: WalRecordKind) -> u64 {
    match kind {
        WalRecordKind::TxBegin => 1,
        WalRecordKind::TxCommit => 2,
        WalRecordKind::TxRollback => 3,
        WalRecordKind::PageAllocate => 4,
        WalRecordKind::PageFormat => 5,
        WalRecordKind::RowInsert => 6,
        WalRecordKind::RowUpdate => 7,
        WalRecordKind::RowDelete => 8,
        WalRecordKind::IndexInsert => 9,
        WalRecordKind::IndexDelete => 10,
        WalRecordKind::MvccVersionCreate => 11,
        WalRecordKind::MvccVersionClose => 12,
        WalRecordKind::MapDeltaAppend => 13,
        WalRecordKind::CheckpointBegin => 14,
        WalRecordKind::CheckpointEnd => 15,
        WalRecordKind::SnapshotBegin => 16,
        WalRecordKind::SnapshotEnd => 17,
        WalRecordKind::ManifestSwitch => 18,
        WalRecordKind::CatalogChangeBegin => 19,
        WalRecordKind::CatalogChangeApply => 20,
        WalRecordKind::CatalogChangeCommit => 21,
        WalRecordKind::SecurityAuditAppend => 22,
    }
}

pub(crate) fn wal_record_kind_from_tag(tag: u64) -> Option<WalRecordKind> {
    match tag {
        1 => Some(WalRecordKind::TxBegin),
        2 => Some(WalRecordKind::TxCommit),
        3 => Some(WalRecordKind::TxRollback),
        4 => Some(WalRecordKind::PageAllocate),
        5 => Some(WalRecordKind::PageFormat),
        6 => Some(WalRecordKind::RowInsert),
        7 => Some(WalRecordKind::RowUpdate),
        8 => Some(WalRecordKind::RowDelete),
        9 => Some(WalRecordKind::IndexInsert),
        10 => Some(WalRecordKind::IndexDelete),
        11 => Some(WalRecordKind::MvccVersionCreate),
        12 => Some(WalRecordKind::MvccVersionClose),
        13 => Some(WalRecordKind::MapDeltaAppend),
        14 => Some(WalRecordKind::CheckpointBegin),
        15 => Some(WalRecordKind::CheckpointEnd),
        16 => Some(WalRecordKind::SnapshotBegin),
        17 => Some(WalRecordKind::SnapshotEnd),
        18 => Some(WalRecordKind::ManifestSwitch),
        19 => Some(WalRecordKind::CatalogChangeBegin),
        20 => Some(WalRecordKind::CatalogChangeApply),
        21 => Some(WalRecordKind::CatalogChangeCommit),
        22 => Some(WalRecordKind::SecurityAuditAppend),
        _ => None,
    }
}

pub fn summarize_transaction<'a>(
    transaction_id: TransactionId,
    records: impl IntoIterator<Item = &'a WalRecord>,
) -> Option<DurableTransactionResume> {
    let mut first_lsn = None;
    let mut last_lsn = None;
    let mut begin_lsn = None;
    let mut commit_lsn = None;
    let mut rollback_lsn = None;
    let mut record_count = 0;

    for record in records {
        if !record.is_for_transaction(transaction_id) {
            continue;
        }

        let lsn = record.header.lsn;
        first_lsn = Some(first_lsn.map_or(lsn, |current: Lsn| current.min(lsn)));
        last_lsn = Some(last_lsn.map_or(lsn, |current: Lsn| current.max(lsn)));
        record_count += 1;

        match record.header.kind {
            WalRecordKind::TxBegin => begin_lsn = Some(lsn),
            WalRecordKind::TxCommit => commit_lsn = Some(lsn),
            WalRecordKind::TxRollback => rollback_lsn = Some(lsn),
            _ => {}
        }
    }

    let first_lsn = first_lsn?;
    let last_lsn = last_lsn?;
    let state = match (begin_lsn, commit_lsn, rollback_lsn) {
        (_, Some(_), _) => DurableTransactionState::Committed,
        (_, _, Some(_)) => DurableTransactionState::RolledBack,
        (Some(_), None, None) => DurableTransactionState::Open,
        (None, None, None) => DurableTransactionState::Incomplete,
    };

    Some(DurableTransactionResume {
        transaction_id,
        state,
        first_lsn,
        last_lsn,
        begin_lsn,
        commit_lsn,
        rollback_lsn,
        record_count,
    })
}

pub fn summarize_transactions_from_records<'a>(
    records: impl IntoIterator<Item = &'a WalRecord>,
) -> Vec<DurableTransactionResume> {
    let mut grouped: BTreeMap<u64, Vec<&WalRecord>> = BTreeMap::new();

    for record in records {
        if let Some(transaction_id) = record.transaction_id() {
            grouped
                .entry(transaction_id.get())
                .or_default()
                .push(record);
        }
    }

    grouped
        .into_iter()
        .filter_map(|(transaction_id, records)| {
            summarize_transaction(TransactionId::new(transaction_id), records)
        })
        .collect()
}

pub fn classify_durable_transactions<'a>(
    records: impl IntoIterator<Item = &'a WalRecord>,
) -> DurableTransactionClassifications {
    let mut classifications = DurableTransactionClassifications::default();

    for summary in summarize_transactions_from_records(records) {
        match summary.state {
            DurableTransactionState::Committed => classifications.committed.push(summary),
            DurableTransactionState::RolledBack => classifications.rolled_back.push(summary),
            DurableTransactionState::Open | DurableTransactionState::Incomplete => {
                classifications.incomplete.push(summary);
            }
        }
    }

    classifications
}

pub fn incomplete_transactions_from_records<'a>(
    records: impl IntoIterator<Item = &'a WalRecord>,
) -> Vec<IncompleteDurableTransaction> {
    summarize_transactions_from_records(records)
        .into_iter()
        .filter(|summary| summary.is_incomplete())
        .map(|summary| IncompleteDurableTransaction {
            transaction_id: summary.transaction_id,
            first_lsn: summary.first_lsn,
            last_lsn: summary.last_lsn,
            record_count: summary.record_count,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wal_taxonomy_marks_transaction_boundaries() {
        assert!(WalRecordKind::TxBegin.is_transaction_boundary());
        assert!(WalRecordKind::TxCommit.is_transaction_boundary());
        assert!(WalRecordKind::TxRollback.is_transaction_boundary());
        assert!(!WalRecordKind::RowInsert.is_transaction_boundary());
    }

    #[test]
    fn wal_record_requires_transaction_id_when_needed() {
        let record = WalRecordHeader {
            kind: WalRecordKind::RowInsert,
            lsn: Lsn::new(1),
            previous_lsn: None,
            transaction_id: None,
            payload_length: 8,
            checksum: 99,
        };

        assert_eq!(
            record.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn in_memory_wal_appends_transaction_boundaries_with_monotonic_lsn() {
        let mut wal = InMemoryWal::new();
        let transaction_id = TransactionId::new(7);

        let begin_lsn = wal.append_tx_begin(transaction_id).unwrap();
        let commit_lsn = wal.append_tx_commit(transaction_id).unwrap();

        assert_eq!(begin_lsn, Lsn::new(1));
        assert_eq!(commit_lsn, Lsn::new(2));
        assert_eq!(wal.next_lsn(), Lsn::new(3));
        assert_eq!(wal.records()[0].header.kind, WalRecordKind::TxBegin);
        assert_eq!(wal.records()[1].header.kind, WalRecordKind::TxCommit);
    }

    #[test]
    fn in_memory_wal_flushes_durable_lsn() {
        let mut wal = InMemoryWal::new();
        let transaction_id = TransactionId::new(8);

        let begin_lsn = wal.append_tx_begin(transaction_id).unwrap();
        let commit_lsn = wal.append_tx_commit(transaction_id).unwrap();

        assert_eq!(wal.durable_lsn(), Lsn::default());
        assert_eq!(wal.flush_through(begin_lsn).unwrap(), begin_lsn);
        assert_eq!(wal.durable_lsn(), begin_lsn);
        assert_eq!(wal.flush_all().unwrap(), commit_lsn);
        assert_eq!(wal.durable_lsn(), commit_lsn);
    }

    #[test]
    fn in_memory_wal_replay_durable_excludes_unflushed_records() {
        let mut wal = InMemoryWal::new();
        let transaction_id = TransactionId::new(9);

        let begin_lsn = wal.append_tx_begin(transaction_id).unwrap();
        let commit_lsn = wal.append_tx_commit(transaction_id).unwrap();
        wal.flush_through(begin_lsn).unwrap();

        let replay = wal.replay_durable();

        assert_eq!(commit_lsn, Lsn::new(2));
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].header.kind, WalRecordKind::TxBegin);
        assert_eq!(replay[0].header.lsn, begin_lsn);
    }

    #[test]
    fn durable_resume_summarizes_transaction_records() {
        let mut wal = InMemoryWal::new();
        let committed_tx = TransactionId::new(11);
        let open_tx = TransactionId::new(12);

        wal.append_tx_begin(committed_tx).unwrap();
        wal.append_payload(WalRecordKind::RowUpdate, Some(committed_tx), b"row-1")
            .unwrap();
        let commit_lsn = wal.append_tx_commit(committed_tx).unwrap();
        wal.append_tx_begin(open_tx).unwrap();
        wal.append_payload(WalRecordKind::RowInsert, Some(open_tx), b"row-2")
            .unwrap();
        wal.flush_all().unwrap();

        let resume = wal.resume_transaction(committed_tx).unwrap();
        assert_eq!(resume.state, DurableTransactionState::Committed);
        assert_eq!(resume.commit_lsn, Some(commit_lsn));
        assert_eq!(resume.record_count, 3);
        assert!(resume.is_complete());

        let incomplete = wal.incomplete_durable_transactions();
        assert_eq!(incomplete.len(), 1);
        assert_eq!(incomplete[0].transaction_id, open_tx);
        assert_eq!(incomplete[0].record_count, 2);
    }

    #[test]
    fn durable_transaction_classification_partitions_terminal_and_incomplete_states() {
        let mut wal = InMemoryWal::new();
        let committed_tx = TransactionId::new(14);
        let rolled_back_tx = TransactionId::new(15);
        let open_tx = TransactionId::new(16);
        let incomplete_tx = TransactionId::new(17);

        wal.append_tx_begin(committed_tx).unwrap();
        wal.append_payload(WalRecordKind::RowInsert, Some(committed_tx), b"commit")
            .unwrap();
        wal.append_tx_commit(committed_tx).unwrap();
        wal.append_tx_begin(rolled_back_tx).unwrap();
        wal.append_payload(WalRecordKind::RowDelete, Some(rolled_back_tx), b"rollback")
            .unwrap();
        wal.append_tx_rollback(rolled_back_tx).unwrap();
        wal.append_tx_begin(open_tx).unwrap();
        wal.append_payload(WalRecordKind::RowUpdate, Some(open_tx), b"open")
            .unwrap();
        wal.append_payload(
            WalRecordKind::RowInsert,
            Some(incomplete_tx),
            b"missing-begin",
        )
        .unwrap();
        wal.flush_all().unwrap();

        let classifications = wal.classify_durable_transactions();

        assert_eq!(
            classifications
                .committed_transaction_ids()
                .collect::<Vec<_>>(),
            vec![committed_tx]
        );
        assert_eq!(
            classifications
                .rolled_back_transaction_ids()
                .collect::<Vec<_>>(),
            vec![rolled_back_tx]
        );
        assert_eq!(
            classifications
                .incomplete_transaction_ids()
                .collect::<Vec<_>>(),
            vec![open_tx, incomplete_tx]
        );
    }

    #[test]
    fn in_memory_wal_rejects_invalid_records() {
        let mut wal = InMemoryWal::new();
        let transaction_id = TransactionId::new(10);

        assert_eq!(
            wal.append_payload(WalRecordKind::TxCommit, None, Vec::new())
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );

        let mut invalid_payload_length = WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(transaction_id),
            Vec::new(),
        )
        .unwrap();
        invalid_payload_length.header.payload_length = 99;

        assert_eq!(
            wal.append(invalid_payload_length).unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );

        let skipped_lsn = WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(2),
            None,
            Some(transaction_id),
            Vec::new(),
        )
        .unwrap();

        assert_eq!(
            wal.append(skipped_lsn).unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn in_memory_wal_rejects_lsn_overflow() {
        let transaction_id = TransactionId::new(13);
        let max_record = WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::MAX,
            Some(Lsn::new(u64::MAX - 1)),
            Some(transaction_id),
            Vec::new(),
        )
        .unwrap();
        let mut wal = InMemoryWal {
            records: vec![max_record],
            durable_lsn: Lsn::ZERO,
        };
        let next_record = WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::MAX,
            Some(Lsn::new(u64::MAX - 1)),
            Some(transaction_id),
            Vec::new(),
        )
        .unwrap();

        assert_eq!(
            wal.try_next_lsn().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
        assert_eq!(
            wal.append(next_record).unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }
}
