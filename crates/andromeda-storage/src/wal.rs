//! WAL domain re-exports for backward compatibility.
//!
//! All implementations have moved to the write_ahead_log module.
//! This file maintains backward compatibility through re-exports.

pub use crate::write_ahead_log::{
    classify_durable_transactions, incomplete_transactions_from_records, summarize_transaction,
    summarize_transactions_from_records, wal_record_checksum, wal_record_kind_from_tag,
    wal_record_kind_tag, DurableTransactionClassifications, DurableTransactionResume,
    DurableTransactionState, InMemoryWal, IncompleteDurableTransaction, WalRecord, WalRecordHeader,
    WalRecordKind,
};

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
}
