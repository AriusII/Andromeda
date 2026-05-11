#![forbid(unsafe_code)]
#![doc = r#"
Andromeda transaction lifecycle state and WAL adapter boundary contracts.

This crate owns storage-agnostic transaction state transitions and the typed
WAL adapter contract used to enforce durable terminal evidence.

C5 invariants:

- A commit must not become visible before its commit WAL record is durable.
- Rollback paths that claim crash-recoverable terminal state require durable evidence.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
"#]

mod allocator;
pub mod commit_log;
pub mod commit_protocol;
mod locking_protocol;
mod manager;
mod state;
mod trace;
pub mod transition_sink;
mod wal_adapter;

pub use allocator::TransactionIdAllocator;
pub use commit_protocol::CommitProtocol;
pub use locking_protocol::{TwoPhaseLocksValidator, TwoPhaseOperation};
pub use manager::{TransactionLockCoordinator, TransactionManager, TransactionRecord};
pub use state::{TransactionEvent, TransactionState, TransactionStateMachine};
pub use trace::{
    LockReleaseAllTrace, TransactionTrace, TransactionTransitionCorrelation, transaction_phase_code,
};
pub use transition_sink::{InMemoryTransitionSink, NullTransitionSink, TransactionTransitionSink};
pub use wal_adapter::{TxWalAdapterTrait, WalManager, append_commit_and_flush};
