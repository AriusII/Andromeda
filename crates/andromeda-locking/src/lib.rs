#![forbid(unsafe_code)]

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
