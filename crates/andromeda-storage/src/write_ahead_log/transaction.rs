//! Compatibility reexports for durable WAL transaction summaries.
//!
//! The canonical definitions moved to
//! `andromeda_wal::write_ahead_log::transaction`. Keep this module as a stable
//! storage import path while callers migrate.

pub use andromeda_wal::write_ahead_log::transaction::{
    DurableTransactionClassifications, DurableTransactionResume, DurableTransactionState,
    IncompleteDurableTransaction, classify_durable_transactions,
    incomplete_transactions_from_records, summarize_transaction,
    summarize_transactions_from_records,
};
