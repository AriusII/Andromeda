//! Compatibility re-exports for transaction/WAL adapter boundaries.
//!
//! The implementation lives in `andromeda-transaction`; `andromeda-tx` keeps
//! this module as the legacy import path while extraction continues.

pub use andromeda_transaction::{
    TxWalAdapterError, TxWalAdapterReplayKind, TxWalAdapterReplayRecord, TxWalAdapterTrait,
    WalManager, append_commit_and_flush, map_tx_wal_replay_records,
};
