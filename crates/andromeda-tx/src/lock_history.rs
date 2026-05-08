//! Compatibility re-exports for lock and transaction history traces.

pub use andromeda_locking::lock_history::{
    DeadlockAuditTrace, DeadlockDecisionKind, LockPromotionTrace, LockWaitTrace,
};
pub use andromeda_transaction::LockReleaseAllTrace;
