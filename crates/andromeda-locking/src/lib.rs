#![forbid(unsafe_code)]

pub mod deadlock_detection;
mod entry;
mod evidence;
pub mod lock_history;
mod manager_core;
mod mode;
mod resource;

use self::manager_core::{validate_non_zero, validate_transaction_id};

pub use self::deadlock_detection::*;
pub use self::entry::{LockEntry, LockHolder, LockWaiter};
pub use self::evidence::{
    LockAcquireEvidence, LockAcquireStatus, LockDecisionEvidence, LockReleaseAllEvidence,
    LockReleaseAllSummary, LockReleaseEvidence, LockTraceKind, LockTraceOutcome,
};
pub use self::lock_history::{
    DeadlockAuditTrace, DeadlockDecisionKind, LockPromotionTrace, LockWaitTrace,
};
pub use self::manager_core::LockManager;
pub use self::mode::LockMode;
pub use self::resource::{CatalogIntentionLocks, LockRequest, LockResource};

#[cfg(test)]
mod tests;
