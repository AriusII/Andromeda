//! Durable transaction state classification and resumption.

use andromeda_core::TransactionId;
use std::collections::BTreeMap;

use super::WalRecord;
use crate::Lsn;

/// Terminal state of a durable transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurableTransactionState {
    Open,
    Committed,
    RolledBack,
    Incomplete,
}

/// Summary of a transaction as it appears in the WAL.
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

/// Incomplete transaction still present in the WAL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncompleteDurableTransaction {
    pub transaction_id: TransactionId,
    pub first_lsn: Lsn,
    pub last_lsn: Lsn,
    pub record_count: usize,
}

/// Partitioned classification of durable transactions.
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

/// Summarize WAL records for a specific transaction.
pub fn summarize_transaction<'a>(
    transaction_id: TransactionId,
    records: impl IntoIterator<Item = &'a WalRecord>,
) -> Option<DurableTransactionResume> {
    use super::WalRecordKind;

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
        (Some(_), Some(_), None) => DurableTransactionState::Committed,
        (Some(_), None, Some(_)) => DurableTransactionState::RolledBack,
        (Some(_), None, None) => DurableTransactionState::Open,
        _ => DurableTransactionState::Incomplete,
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

/// Group WAL records by transaction and summarize each.
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

/// Classify durable transactions by terminal state.
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

/// Extract incomplete transactions from WAL records.
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
    use crate::WalRecordKind;
    use andromeda_core::TransactionId;

    #[test]
    fn durable_resume_classifies_committed_and_incomplete() {
        let mut records = Vec::new();
        let committed_tx = TransactionId::new(11);
        let open_tx = TransactionId::new(12);

        // Build committed transaction
        records.push(
            WalRecord::from_parts(
                WalRecordKind::TxBegin,
                Lsn::new(1),
                None,
                Some(committed_tx),
                Vec::new(),
            )
            .unwrap(),
        );
        records.push(
            WalRecord::from_parts(
                WalRecordKind::RowUpdate,
                Lsn::new(2),
                Some(Lsn::new(1)),
                Some(committed_tx),
                b"row-1".to_vec(),
            )
            .unwrap(),
        );
        records.push(
            WalRecord::from_parts(
                WalRecordKind::TxCommit,
                Lsn::new(3),
                Some(Lsn::new(2)),
                Some(committed_tx),
                Vec::new(),
            )
            .unwrap(),
        );

        // Build open transaction
        records.push(
            WalRecord::from_parts(
                WalRecordKind::TxBegin,
                Lsn::new(4),
                Some(Lsn::new(3)),
                Some(open_tx),
                Vec::new(),
            )
            .unwrap(),
        );
        records.push(
            WalRecord::from_parts(
                WalRecordKind::RowInsert,
                Lsn::new(5),
                Some(Lsn::new(4)),
                Some(open_tx),
                b"row-2".to_vec(),
            )
            .unwrap(),
        );

        let classifications = classify_durable_transactions(&records);
        assert_eq!(classifications.committed.len(), 1);
        assert_eq!(classifications.incomplete.len(), 1);
    }

    #[test]
    fn durable_resume_requires_begin_before_terminal_commit() {
        let transaction_id = TransactionId::new(13);
        let records = vec![
            WalRecord::from_parts(
                WalRecordKind::RowUpdate,
                Lsn::new(1),
                None,
                Some(transaction_id),
                b"row-without-begin".to_vec(),
            )
            .unwrap(),
            WalRecord::from_parts(
                WalRecordKind::TxCommit,
                Lsn::new(2),
                Some(Lsn::new(1)),
                Some(transaction_id),
                Vec::new(),
            )
            .unwrap(),
        ];

        let summary = summarize_transaction(transaction_id, &records).unwrap();

        assert_eq!(summary.state, DurableTransactionState::Incomplete);
        assert!(summary.begin_lsn.is_none());
        assert_eq!(summary.commit_lsn, Some(Lsn::new(2)));
    }

    #[test]
    fn durable_resume_treats_conflicting_terminals_as_incomplete() {
        let transaction_id = TransactionId::new(14);
        let records = vec![
            WalRecord::from_parts(
                WalRecordKind::TxBegin,
                Lsn::new(1),
                None,
                Some(transaction_id),
                Vec::new(),
            )
            .unwrap(),
            WalRecord::from_parts(
                WalRecordKind::TxCommit,
                Lsn::new(2),
                Some(Lsn::new(1)),
                Some(transaction_id),
                Vec::new(),
            )
            .unwrap(),
            WalRecord::from_parts(
                WalRecordKind::TxRollback,
                Lsn::new(3),
                Some(Lsn::new(2)),
                Some(transaction_id),
                Vec::new(),
            )
            .unwrap(),
        ];

        let summary = summarize_transaction(transaction_id, &records).unwrap();

        assert_eq!(summary.state, DurableTransactionState::Incomplete);
        assert_eq!(summary.commit_lsn, Some(Lsn::new(2)));
        assert_eq!(summary.rollback_lsn, Some(Lsn::new(3)));
    }
}
