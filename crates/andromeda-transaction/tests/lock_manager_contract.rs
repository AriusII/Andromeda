//! Transaction/locking orchestration contracts.
//!
//! Pure lock table behavior lives in `andromeda-locking`; this target keeps
//! only tests that require `TransactionManager` state and coordinator APIs.

use andromeda_error::{AndromedaErrorKind, AndromedaResult};
use andromeda_locking::{
    LockAcquireStatus, LockHolder, LockManager, LockMode, LockReleaseAllSummary, LockResource,
};
use andromeda_transaction::{TransactionManager, TransactionState};
use andromeda_types::TransactionId;

#[path = "lock_manager_contract/coordinator_contracts.rs"]
mod coordinator_contracts;
#[path = "lock_manager_contract/fixtures.rs"]
mod fixtures;
#[path = "lock_manager_contract/strict_2pl_contracts.rs"]
mod strict_2pl_contracts;
#[path = "lock_manager_contract/transaction_coordinator_cleanup_contracts.rs"]
mod transaction_coordinator_cleanup_contracts;
