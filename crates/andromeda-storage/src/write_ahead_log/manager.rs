//! In-memory WAL manager for record accumulation and flushing.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

use super::transaction::{
    classify_durable_transactions, incomplete_transactions_from_records, summarize_transaction,
    DurableTransactionClassifications, DurableTransactionResume, IncompleteDurableTransaction,
};
use super::{WalRecord, WalRecordKind};
use crate::Lsn;

/// In-memory WAL accumulator with durable LSN tracking.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_wal_appends_transaction_boundaries() {
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
    fn in_memory_wal_replay_durable_excludes_unflushed() {
        let mut wal = InMemoryWal::new();
        let transaction_id = TransactionId::new(9);

        let begin_lsn = wal.append_tx_begin(transaction_id).unwrap();
        let commit_lsn = wal.append_tx_commit(transaction_id).unwrap();
        wal.flush_through(begin_lsn).unwrap();

        let replay = wal.replay_durable();

        assert_eq!(commit_lsn, Lsn::new(2));
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].header.kind, WalRecordKind::TxBegin);
    }
}
