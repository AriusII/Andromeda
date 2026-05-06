//! Lock manager API.
//!
//! This module defines the storage-agnostic lock vocabulary and a minimal,
//! panic-free manager. C3-LM-002 defines the V0 lock-mode compatibility matrix;
//! C3-LM-003 adds nonblocking acquire decisions. C3-LM-004 adds conservative
//! same-transaction re-entry and nonblocking upgrade rules. C3-LM-005 adds
//! deterministic FIFO waiter promotion on release. C3-LM-006 adds terminal
//! transaction cleanup through strict 2PL `release_all`; it is only for the
//! boundary after a durable commit decision or durable rollback decision is
//! known, and must not be used for early lock release. Deadlock detection
//! remains deferred.

mod entry;
mod evidence;
mod manager_core;
mod mode;
mod resource;

use self::manager_core::{validate_non_zero, validate_transaction_id};

pub use self::entry::{LockEntry, LockHolder, LockWaiter};
pub use self::evidence::{
    LockAcquireEvidence, LockAcquireStatus, LockDecisionEvidence, LockReleaseAllEvidence,
    LockReleaseAllSummary, LockReleaseEvidence, LockTraceKind, LockTraceOutcome,
};
pub use self::manager_core::LockManager;
pub use self::mode::LockMode;
pub use self::resource::{CatalogIntentionLocks, LockRequest, LockResource};

#[cfg(test)]
mod tests;
