#![forbid(unsafe_code)]
#![doc = r#"
Logical transaction terminal evidence for Andromeda.

This crate owns transaction-log record shapes, transaction-local LSN evidence,
and replay-facing terminal classification. It does not own physical WAL bytes or
commit visibility publication.

C5 invariants:

- A commit must not become visible before its commit WAL record is durable.
- Terminal transaction evidence must be read from a verified durable WAL prefix.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
"#]

mod adapter_replay;
mod entry;
mod error;
mod lsn;
mod replay;
mod rollback;
mod status_rebuild;
mod wal;

pub use adapter_replay::{
    TxWalAdapterReplayKind, TxWalAdapterReplayRecord, map_tx_wal_replay_records,
};
pub use entry::{CommitLogEntry, IsolationLevel, WalRecordKind};
pub use error::TxWalAdapterError;
pub use lsn::Lsn;
pub use replay::{TxWalReplayAction, TxWalReplayRecord, TxWalReplaySummary};
pub use rollback::RollbackLogEntry;
pub use status_rebuild::TransactionStatusRebuild;
pub use wal::{
    InvocationWal, InvocationWalFuture, TX_COMMIT_PAYLOAD_LEN, TX_ROLLBACK_PAYLOAD_LEN,
    encode_commit_payload, encode_rollback_payload,
};
