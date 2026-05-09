//! Lock manager contract tests split by behavior family.
//!
//! The root keeps only shared imports and module routing; contract assertions
//! live in focused files under `tests/lock_manager_contract/`.

use andromeda_error::{AndromedaErrorKind, AndromedaResult};

use andromeda_locking::{
    DeadlockDecisionTraceOutcome, DeadlockPolicy, DeadlockVictimPolicy, LockAcquireStatus,
    LockHolder, LockManager, LockMode, LockReleaseAllSummary, LockResource, LockTraceKind,
    LockTraceOutcome, decide_deadlock_from_lock_manager,
};
use andromeda_transaction::{TransactionManager, TransactionState};
use andromeda_types::TransactionId;

#[path = "lock_manager_contract/acquisition_wait_fairness_contracts.rs"]
mod acquisition_wait_fairness_contracts;
#[path = "lock_manager_contract/cleanup_snapshot_contracts.rs"]
mod cleanup_snapshot_contracts;
#[path = "lock_manager_contract/compatibility_contracts.rs"]
mod compatibility_contracts;
#[path = "lock_manager_contract/coordinator_contracts.rs"]
mod coordinator_contracts;
#[path = "lock_manager_contract/deadlock_timeout_contracts.rs"]
mod deadlock_timeout_contracts;
#[path = "lock_manager_contract/evidence_contracts.rs"]
mod evidence_contracts;
#[path = "lock_manager_contract/fixtures.rs"]
mod fixtures;
#[path = "lock_manager_contract/strict_2pl_contracts.rs"]
mod strict_2pl_contracts;
#[path = "lock_manager_contract/trace_boundary_contracts.rs"]
mod trace_boundary_contracts;
#[path = "lock_manager_contract/transaction_coordinator_cleanup_contracts.rs"]
mod transaction_coordinator_cleanup_contracts;
