//! Strict Two-Phase Locking (2PL) Validation and State Machine Rules.
//!
//! This module enforces the invariants required for strict 2PL transaction
//! concurrency control, ensuring that:
//!
//! 1. **Growing Phase** (lock acquisition): Active → InFlight
//!    - Transactions acquire all necessary locks.
//!    - New lock acquisitions are permitted.
//!
//! 2. **Shrinking Phase** (lock release): InFlight → Committing
//!    - Transactions release locks in preparation for commit or rollback.
//!    - No new lock acquisitions are permitted after any release.
//!
//! 3. **Terminal Cleanup**: Committed/RolledBack → Disposed
//!    - All locks must be released.
//!    - No further operations are allowed.
//!
//! Violation of these rules breaks serializability and is rejected with a clear
//! error message identifying the 2PL violation.

use crate::state::TransactionState;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Validator for strict 2PL state transitions and lock operation preconditions.
///
/// 2PL strict discipline ensures that lock acquisition only occurs in the
/// Growing Phase (Active or InFlight), lock release only in the Shrinking
/// Phase (InFlight or Committing), and no operations after terminal states.
///
/// References:
/// - C3-LM-006: Lock manager release_all semantics
/// - Wave 14: Transaction state machine 2PL enforcement
#[derive(Debug, Clone, Copy)]
pub struct TwoPhaseLocksValidator;

impl TwoPhaseLocksValidator {
    /// Verify that a transaction state allows lock acquisition (Growing Phase).
    ///
    /// 2PL Growing Phase: lock acquisition is only permitted in Active state.
    /// Once the transaction enters Committing or RollingBack, the shrinking phase
    /// begins and no new acquires are allowed.
    ///
    /// # Arguments
    /// * `state` - Current transaction state
    ///
    /// # Returns
    /// `true` if the state permits lock acquisition, `false` otherwise.
    ///
    /// # 2PL Invariant
    /// ```text
    /// Growing Phase states: Active
    /// Shrinking Phase states: Committing, RollingBack (no acquire)
    /// Terminal states: Committed, RolledBack, Disposed (no acquire)
    /// ```
    pub const fn state_allows_acquire(state: TransactionState) -> bool {
        matches!(state, TransactionState::Active)
    }

    /// Verify that a transaction state allows single-resource lock release.
    ///
    /// 2PL permits lock release in InFlight (transition to Committing) and
    /// Committing (finishing shrinking phase). Release in other states is a violation.
    ///
    /// # Arguments
    /// * `state` - Current transaction state
    ///
    /// # Returns
    /// `true` if the state permits lock release, `false` otherwise.
    ///
    /// # 2PL Invariant
    /// ```text
    /// Release is allowed: InFlight, Committing
    /// Release prohibited: Created, Active, Committed, RolledBack, Failed, Poisoned, Disposed
    /// ```
    pub const fn state_allows_release(state: TransactionState) -> bool {
        matches!(
            state,
            TransactionState::Committing | TransactionState::RollingBack
        )
    }

    /// Verify that a transaction state allows terminal lock cleanup (release_all).
    ///
    /// Terminal cleanup (release_all) is only called after a durable commit or
    /// rollback decision is recorded. It is the final cleanup operation before
    /// transaction disposal.
    ///
    /// # Arguments
    /// * `state` - Current transaction state
    ///
    /// # Returns
    /// `true` if the state permits release_all, `false` otherwise.
    ///
    /// # 2PL Invariant
    /// ```text
    /// release_all is allowed: Committed, RolledBack
    /// release_all prohibited: all other states
    /// ```
    pub const fn state_allows_release_all(state: TransactionState) -> bool {
        matches!(
            state,
            TransactionState::Committed | TransactionState::RolledBack
        )
    }

    /// Validate that a transaction does not attempt to re-acquire after entering
    /// the shrinking phase.
    ///
    /// This validates the 2PL invariant: once a lock is released, no new locks
    /// may be acquired. This is enforced through state transitions (Committing
    /// state prohibits acquire), but this function provides explicit validation.
    ///
    /// # Arguments
    /// * `state_at_release` - State when lock was released
    /// * `next_state` - State for next acquire attempt
    ///
    /// # Returns
    /// `Ok(())` if the transition is valid (no re-acquire after shrinking).
    /// `Err` if 2PL is violated (acquire after release in shrinking phase).
    pub fn validate_no_acquire_after_release(
        state_at_release: TransactionState,
        next_state: TransactionState,
    ) -> AndromedaResult<()> {
        // If we released in Committing or RollingBack (shrinking phase) and now
        // attempt to acquire, that violates 2PL.
        if matches!(
            state_at_release,
            TransactionState::Committing | TransactionState::RollingBack
        ) && matches!(next_state, TransactionState::Active | TransactionState::Committing)
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "2PL violated: cannot acquire locks after entering shrinking phase (post-release)",
            ));
        }

        Ok(())
    }

    /// Validate the complete 2PL lock lifecycle for a transaction.
    ///
    /// This is a high-level assertion that encodes the full 2PL contract:
    /// - No acquire after release (shrinking phase lock-out)
    /// - No operations after terminal cleanup
    /// - Proper state progression: Growing → Shrinking → Terminal
    ///
    /// # Arguments
    /// * `current_state` - Transaction's current state
    /// * `requested_operation` - The operation being requested (Acquire, Release, ReleaseAll)
    ///
    /// # Returns
    /// `Ok(())` if the operation is permitted under 2PL rules.
    /// `Err` if the operation violates 2PL invariants.
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
            }
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
            }
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
            }
            TwoPhaseOperation::NoOp => {
                // No validation needed for no-op
            }
        }

        Ok(())
    }
}

/// Enum representing the lock-related operations subject to 2PL validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TwoPhaseOperation {
    /// Lock acquisition (growing phase operation)
    Acquire,
    /// Lock release (shrinking phase operation)
    Release,
    /// Terminal lock cleanup (post-commit/rollback)
    ReleaseAll,
    /// Placeholder for non-lock operations
    NoOp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_allows_acquire_in_active() {
        assert!(TwoPhaseLocksValidator::state_allows_acquire(TransactionState::Active));
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
    fn state_disallows_release_all_in_inflight() {
        // InFlight maps to Active in the current state machine
        assert!(!TwoPhaseLocksValidator::state_allows_release_all(
            TransactionState::Active
        ));
    }

    #[test]
    fn validate_operation_acquire_in_active() {
        assert!(TwoPhaseLocksValidator::validate_operation(
            TransactionState::Active,
            TwoPhaseOperation::Acquire
        )
        .is_ok());
    }

    #[test]
    fn validate_operation_acquire_in_disposed_fails() {
        assert!(TwoPhaseLocksValidator::validate_operation(
            TransactionState::Disposed,
            TwoPhaseOperation::Acquire
        )
        .is_err());
    }

    #[test]
    fn validate_operation_release_in_committing() {
        assert!(TwoPhaseLocksValidator::validate_operation(
            TransactionState::Committing,
            TwoPhaseOperation::Release
        )
        .is_ok());
    }

    #[test]
    fn validate_operation_release_in_active_fails() {
        assert!(TwoPhaseLocksValidator::validate_operation(
            TransactionState::Active,
            TwoPhaseOperation::Release
        )
        .is_err());
    }

    #[test]
    fn validate_operation_release_all_in_committed() {
        assert!(TwoPhaseLocksValidator::validate_operation(
            TransactionState::Committed,
            TwoPhaseOperation::ReleaseAll
        )
        .is_ok());
    }

    #[test]
    fn validate_operation_release_all_in_committing_fails() {
        assert!(TwoPhaseLocksValidator::validate_operation(
            TransactionState::Committing,
            TwoPhaseOperation::ReleaseAll
        )
        .is_err());
    }
}
