//! Legacy crate-root WAL compatibility surface.
//!
//! Pure WAL records, transaction summaries, and in-memory WAL management now
//! live in `andromeda_wal`. Storage keeps these reexports so existing
//! `andromeda_storage::*` import paths continue to resolve during migration.

pub use andromeda_wal::{
    DurableTransactionClassifications, DurableTransactionResume, DurableTransactionState,
    InMemoryWal, IncompleteDurableTransaction, MemoryWal, WalRecord, WalRecordHeader,
    WalRecordKind, classify_durable_transactions, incomplete_transactions_from_records,
    summarize_transaction, summarize_transactions_from_records, wal_record_checksum,
    wal_record_kind_from_tag, wal_record_kind_tag,
};
