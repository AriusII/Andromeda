//! Transaction manager that owns live state machines and the status table.
//!
//! The [`TransactionManager`] is the single production entry point for
//! beginning, committing, rolling back, and poisoning transactions. It
//! enforces the invariants encoded in [`TransactionStateMachine`] and mirrors
//! every terminal transition into the [`TransactionStatusTable`] used by MVCC
//! visibility.
//!
//! Higher-level concerns (WAL replay, MVCC snapshot construction) are
//! deliberately out of scope. Lock management is exposed only through a narrow,
//! boundary-safe coordinator/facade that validates transaction membership and
//! delegates to the lock manager without changing commit or rollback semantics.

mod lock_coordinator;
mod manager_core;
mod record;

pub use lock_coordinator::TransactionLockCoordinator;
pub use manager_core::TransactionManager;
pub use record::TransactionRecord;
