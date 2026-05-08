//! Strict Two-Phase Locking (2PL) validation and state machine rules.
//!
//! This module enforces the invariants required for strict 2PL transaction
//! concurrency control, ensuring that:
//!
//! 1. **Growing phase** (lock acquisition): Active
//!    - Transactions acquire all necessary locks.
//!    - New lock acquisitions are permitted.
//!
//! 2. **Shrinking phase** (lock release): Committing or RollingBack
//!    - Transactions release locks in preparation for commit or rollback.
//!    - No new lock acquisitions are permitted after any release.
//!
//! 3. **Terminal cleanup**: Committed or RolledBack
//!    - All locks must be released.
//!    - No further operations are allowed.
//!
//! Violation of these rules breaks serializability and is rejected with a clear
//! error message identifying the 2PL violation.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::TransactionState;

/// Validator for strict 2PL state transitions and lock operation preconditions.
///
/// 2PL strict discipline ensures that lock acquisition only occurs in the
/// growing phase (`Active`), lock release only in the shrinking phase
/// (`Committing` or `RollingBack`), and no operations after terminal states.
///
/// References:
/// - C3-LM-006: Lock manager release_all semantics
/// - Transaction state machine strict 2PL enforcement
#[derive(Debug, Clone, Copy)]
pub struct TwoPhaseLocksValidator;

impl TwoPhaseLocksValidator {
    /// Verify that a transaction state allows lock acquisition.
    pub const fn state_allows_acquire(state: TransactionState) -> bool {
        matches!(state, TransactionState::Active)
    }

    /// Verify that a transaction state allows single-resource lock release.
    pub const fn state_allows_release(state: TransactionState) -> bool {
        matches!(
            state,
            TransactionState::Committing | TransactionState::RollingBack
        )
    }

    /// Verify that a transaction state allows terminal lock cleanup (`release_all`).
    pub const fn state_allows_release_all(state: TransactionState) -> bool {
        matches!(
            state,
            TransactionState::Committed | TransactionState::RolledBack
        )
    }

    /// Validate that a transaction does not attempt to re-acquire after
    /// entering the shrinking phase.
    pub fn validate_no_acquire_after_release(
        state_at_release: TransactionState,
        next_state: TransactionState,
    ) -> AndromedaResult<()> {
        if matches!(
            state_at_release,
            TransactionState::Committing | TransactionState::RollingBack
        ) && matches!(
            next_state,
            TransactionState::Active | TransactionState::Committing
        ) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "2PL violated: cannot acquire locks after entering shrinking phase (post-release)",
            ));
        }

        Ok(())
    }

    /// Validate the complete 2PL lock lifecycle for a transaction operation.
    pub fn validate_operation(
        current_state: TransactionState,
        requested_operation: TwoPhaseOperation,
    ) -> AndromedaResult<()> {
        match requested_operation {
            TwoPhaseOperation::Acquire => {
                if !Self::state_allows_acquire(current_state) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        format!(
                            "cannot acquire lock in {:?} state; 2PL requires Active or Committing",
                            current_state
                        ),
                    ));
                }
            },
            TwoPhaseOperation::Release => {
                if !Self::state_allows_release(current_state) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        format!(
                            "cannot release lock in {:?} state; 2PL requires Committing or RollingBack",
                            current_state
                        ),
                    ));
                }
            },
            TwoPhaseOperation::ReleaseAll => {
                if !Self::state_allows_release_all(current_state) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        format!(
                            "cannot release_all in {:?} state; 2PL requires Committed or RolledBack",
                            current_state
                        ),
                    ));
                }
            },
            TwoPhaseOperation::NoOp => {},
        }

        Ok(())
    }
}

/// Lock-related operations subject to 2PL validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TwoPhaseOperation {
    /// Lock acquisition during the growing phase.
    Acquire,
    /// Single-resource lock release during the shrinking phase.
    Release,
    /// Terminal lock cleanup after durable commit or rollback evidence.
    ReleaseAll,
    /// Placeholder for non-lock operations.
    NoOp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_allows_acquire_in_active() {
        assert!(TwoPhaseLocksValidator::state_allows_acquire(
            TransactionState::Active
        ));
    }

    #[test]
    fn state_disallows_acquire_in_committing() {
        assert!(!TwoPhaseLocksValidator::state_allows_acquire(
            TransactionState::Committing
        ));
    }

    #[test]
    fn state_disallows_acquire_in_created() {
        assert!(!TwoPhaseLocksValidator::state_allows_acquire(
            TransactionState::Created
        ));
    }

    #[test]
    fn state_disallows_acquire_in_committed() {
        assert!(!TwoPhaseLocksValidator::state_allows_acquire(
            TransactionState::Committed
        ));
    }

    #[test]
    fn state_disallows_acquire_in_disposed() {
        assert!(!TwoPhaseLocksValidator::state_allows_acquire(
            TransactionState::Disposed
        ));
    }

    #[test]
    fn state_allows_release_in_committing() {
        assert!(TwoPhaseLocksValidator::state_allows_release(
            TransactionState::Committing
        ));
    }

    #[test]
    fn state_allows_release_in_rolling_back() {
        assert!(TwoPhaseLocksValidator::state_allows_release(
            TransactionState::RollingBack
        ));
    }

    #[test]
    fn state_disallows_release_in_active() {
        assert!(!TwoPhaseLocksValidator::state_allows_release(
            TransactionState::Active
        ));
    }

    #[test]
    fn state_disallows_release_in_committed() {
        assert!(!TwoPhaseLocksValidator::state_allows_release(
            TransactionState::Committed
        ));
    }

    #[test]
    fn state_allows_release_all_in_committed() {
        assert!(TwoPhaseLocksValidator::state_allows_release_all(
            TransactionState::Committed
        ));
    }

    #[test]
    fn state_allows_release_all_in_rolled_back() {
        assert!(TwoPhaseLocksValidator::state_allows_release_all(
            TransactionState::RolledBack
        ));
    }

    #[test]
    fn state_disallows_release_all_in_committing() {
        assert!(!TwoPhaseLocksValidator::state_allows_release_all(
            TransactionState::Committing
        ));
    }

    #[test]
    fn state_disallows_release_all_in_active() {
        assert!(!TwoPhaseLocksValidator::state_allows_release_all(
            TransactionState::Active
        ));
    }

    #[test]
    fn validate_operation_acquire_in_active() {
        assert!(
            TwoPhaseLocksValidator::validate_operation(
                TransactionState::Active,
                TwoPhaseOperation::Acquire
            )
            .is_ok()
        );
    }

    #[test]
    fn validate_operation_acquire_in_disposed_fails() {
        assert!(
            TwoPhaseLocksValidator::validate_operation(
                TransactionState::Disposed,
                TwoPhaseOperation::Acquire
            )
            .is_err()
        );
    }

    #[test]
    fn validate_operation_release_in_committing() {
        assert!(
            TwoPhaseLocksValidator::validate_operation(
                TransactionState::Committing,
                TwoPhaseOperation::Release
            )
            .is_ok()
        );
    }

    #[test]
    fn validate_operation_release_in_active_fails() {
        assert!(
            TwoPhaseLocksValidator::validate_operation(
                TransactionState::Active,
                TwoPhaseOperation::Release
            )
            .is_err()
        );
    }

    #[test]
    fn validate_operation_release_all_in_committed() {
        assert!(
            TwoPhaseLocksValidator::validate_operation(
                TransactionState::Committed,
                TwoPhaseOperation::ReleaseAll
            )
            .is_ok()
        );
    }

    #[test]
    fn validate_operation_release_all_in_committing_fails() {
        assert!(
            TwoPhaseLocksValidator::validate_operation(
                TransactionState::Committing,
                TwoPhaseOperation::ReleaseAll
            )
            .is_err()
        );
    }
}
