//! F3 Membership State Machine — Pure State Transitions
//!
//! This module owns the deterministic state machine for replica membership lifecycle.
//! It models the transitions a replica undergoes as it joins, operates, suspects,
//! and eventually is removed or promoted to primary.
//!
//! # State Lifecycle
//!
//! ```text
//! Initial
//!   ↓ (on heartbeat/hello received)
//! Active
//!   ├─ (on missed heartbeat threshold)
//!   ├─→ Suspect
//!   │    ├─ (on heartbeat resumed)
//!   │    └─→ Active
//!   │    ├─ (on dead threshold)
//!   │    └─→ Removed
//!   │
//!   ├─ (on promotion eligibility met)
//!   └─→ Candidate (staged for promotion)
//!        ├─ (on promotion success)
//!        └─→ Promoted
//!        ├─ (on promotion rejection)
//!        └─→ Active
//!
//! Removed (terminal)
//! Promoted (terminal, primary not replica)
//! ```
//!
//! # Key Properties
//!
//! - **Pure Transitions:** No I/O, no async. Caller orchestrates I/O.
//! - **Deterministic:** Same input → same output, every time.
//! - **Replay-Safe:** State machines can be replayed to reconstruct history.
//! - **Atomic:** Each transition is a single atomic step; no partial updates.

use crate::Lsn;

/// Membership state of a replica in the quorum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MembershipState {
    /// Initial state: replica has been registered but not yet observed.
    Initial,
    /// Active: replica is connected, responsive, and participating.
    Active,
    /// Suspect: replica has missed heartbeat(s) but not yet pronounced dead.
    Suspect,
    /// Removed: replica is no longer part of active membership (terminal).
    Removed,
    /// Candidate: replica is staged for promotion attempt.
    Candidate,
    /// Promoted: replica has been successfully promoted to primary (terminal).
    Promoted,
}

impl MembershipState {
    pub const fn is_initial(self) -> bool {
        matches!(self, Self::Initial)
    }

    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }

    pub const fn is_suspect(self) -> bool {
        matches!(self, Self::Suspect)
    }

    pub const fn is_removed(self) -> bool {
        matches!(self, Self::Removed)
    }

    pub const fn is_candidate(self) -> bool {
        matches!(self, Self::Candidate)
    }

    pub const fn is_promoted(self) -> bool {
        matches!(self, Self::Promoted)
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Removed | Self::Promoted)
    }
}

/// Event that triggers a state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionEvent {
    /// Replica sent a hello or heartbeat message.
    HeartbeatReceived,
    /// Replica missed heartbeat threshold (N consecutive missed).
    HeartbeatMissed,
    /// Replica exceeded dead threshold (N+M consecutive missed).
    DeadThresholdExceeded,
    /// Replica's received_lsn advanced.
    LsnAdvanced(Lsn),
    /// Replica is eligible for promotion (caught up to primary_durable_lsn).
    PromotionEligible,
    /// Promotion attempt staged.
    PromotionStaged,
    /// Promotion succeeded at a new epoch.
    PromotionSucceeded,
    /// Promotion failed; remains as replica.
    PromotionFailed,
    /// Operator or fencing engine marks replica for removal.
    RemovalRequested,
}

/// Error type for invalid state transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionError {
    /// The event is not valid in the current state.
    InvalidEventForState,
    /// The state is already terminal.
    AlreadyTerminal,
    /// Precondition not met (e.g., promotion without eligibility).
    PreconditionFailed,
}

impl TransitionError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidEventForState => "event not valid in current state",
            Self::AlreadyTerminal => "state is already terminal",
            Self::PreconditionFailed => "precondition not met for transition",
        }
    }
}

/// Pure state transition function.
///
/// Given a current state and an event, compute the next state.
/// This function is deterministic and has no side effects.
///
/// # Arguments
///
/// * `current` — Current membership state.
/// * `event` — Triggering event.
///
/// # Returns
///
/// Next state on success, or `TransitionError` if the event is invalid.
///
/// # State Machine Rules
///
/// - **Initial** → Active (on HeartbeatReceived)
/// - **Active** → Suspect (on HeartbeatMissed)
/// - **Active** → Candidate (on PromotionStaged, if eligible)
/// - **Suspect** → Active (on HeartbeatReceived)
/// - **Suspect** → Removed (on DeadThresholdExceeded)
/// - **Candidate** → Active (on PromotionFailed)
/// - **Candidate** → Promoted (on PromotionSucceeded)
/// - **Any** → Removed (on RemovalRequested)
/// - **Terminal states** reject further transitions
pub fn transition(
    current: MembershipState,
    event: TransitionEvent,
) -> Result<MembershipState, TransitionError> {
    // Terminal states reject all transitions except for logging/auditing.
    if current.is_terminal() {
        return Err(TransitionError::AlreadyTerminal);
    }

    use MembershipState::*;
    use TransitionEvent::*;

    match (current, event) {
        // From Initial state
        (Initial, HeartbeatReceived) => Ok(Active),
        (Initial, RemovalRequested) => Ok(Removed),

        // From Active state
        (Active, HeartbeatMissed) => Ok(Suspect),
        (Active, PromotionStaged) => Ok(Candidate),
        (Active, RemovalRequested) => Ok(Removed),
        (Active, LsnAdvanced(_)) => Ok(Active), // No state change, stays active
        (Active, HeartbeatReceived) => Ok(Active), // Heartbeat keeps it active

        // From Suspect state
        (Suspect, HeartbeatReceived) => Ok(Active),
        (Suspect, DeadThresholdExceeded) => Ok(Removed),
        (Suspect, RemovalRequested) => Ok(Removed),

        // From Candidate state
        (Candidate, PromotionSucceeded) => Ok(Promoted),
        (Candidate, PromotionFailed) => Ok(Active),
        (Candidate, RemovalRequested) => Ok(Removed),

        // Invalid transitions
        _ => Err(TransitionError::InvalidEventForState),
    }
}

/// Stateful transition wrapper that tracks transition history and validates
/// preconditions.
///
/// This struct is immutable and cheap to clone. It records the full transition
/// history for audit and replay purposes.
#[derive(Debug, Clone)]
pub struct MembershipStateTracker {
    /// Replica ID.
    replica_id: u64,
    /// Current state.
    current_state: MembershipState,
    /// History of all transitions (state, event, epoch).
    history: Vec<StateTransitionRecord>,
}

/// Record of a single state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateTransitionRecord {
    /// Previous state.
    pub from_state: MembershipState,
    /// Event that triggered transition.
    pub event: TransitionEvent,
    /// New state.
    pub to_state: MembershipState,
    /// Membership epoch when transition occurred (for audit).
    pub epoch: u64,
}

impl MembershipStateTracker {
    /// Create a new tracker starting in Initial state.
    pub const fn new(replica_id: u64) -> Self {
        Self {
            replica_id,
            current_state: MembershipState::Initial,
            history: Vec::new(),
        }
    }

    /// Get replica ID.
    pub const fn replica_id(&self) -> u64 {
        self.replica_id
    }

    /// Get current state.
    pub const fn current(&self) -> MembershipState {
        self.current_state
    }

    /// Attempt a state transition. Modifies self; returns the new state or error.
    pub fn try_transition(
        &mut self,
        event: TransitionEvent,
        epoch: u64,
    ) -> Result<MembershipState, TransitionError> {
        let next_state = transition(self.current_state, event)?;
        self.history.push(StateTransitionRecord {
            from_state: self.current_state,
            event,
            to_state: next_state,
            epoch,
        });
        self.current_state = next_state;
        Ok(next_state)
    }

    /// Get transition history (immutable reference).
    pub fn history(&self) -> &[StateTransitionRecord] {
        &self.history
    }

    /// Count transitions of a specific type.
    pub fn transition_count(&self, event_type: TransitionEvent) -> usize {
        self.history
            .iter()
            .filter(|record| record.event == event_type)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_to_active_on_heartbeat() {
        let result = transition(MembershipState::Initial, TransitionEvent::HeartbeatReceived);
        assert_eq!(result, Ok(MembershipState::Active));
    }

    #[test]
    fn active_to_suspect_on_missed_heartbeat() {
        let result = transition(MembershipState::Active, TransitionEvent::HeartbeatMissed);
        assert_eq!(result, Ok(MembershipState::Suspect));
    }

    #[test]
    fn suspect_to_active_on_heartbeat() {
        let result = transition(MembershipState::Suspect, TransitionEvent::HeartbeatReceived);
        assert_eq!(result, Ok(MembershipState::Active));
    }

    #[test]
    fn suspect_to_removed_on_dead_threshold() {
        let result = transition(
            MembershipState::Suspect,
            TransitionEvent::DeadThresholdExceeded,
        );
        assert_eq!(result, Ok(MembershipState::Removed));
    }

    #[test]
    fn active_to_candidate_on_promotion_staged() {
        let result = transition(MembershipState::Active, TransitionEvent::PromotionStaged);
        assert_eq!(result, Ok(MembershipState::Candidate));
    }

    #[test]
    fn candidate_to_promoted_on_success() {
        let result = transition(
            MembershipState::Candidate,
            TransitionEvent::PromotionSucceeded,
        );
        assert_eq!(result, Ok(MembershipState::Promoted));
    }

    #[test]
    fn candidate_to_active_on_promotion_failed() {
        let result = transition(MembershipState::Candidate, TransitionEvent::PromotionFailed);
        assert_eq!(result, Ok(MembershipState::Active));
    }

    #[test]
    fn removal_requested_from_any_non_terminal() {
        for state in &[
            MembershipState::Initial,
            MembershipState::Active,
            MembershipState::Suspect,
            MembershipState::Candidate,
        ] {
            let result = transition(*state, TransitionEvent::RemovalRequested);
            assert_eq!(result, Ok(MembershipState::Removed));
        }
    }

    #[test]
    fn terminal_states_reject_transitions() {
        for state in &[MembershipState::Removed, MembershipState::Promoted] {
            let result = transition(*state, TransitionEvent::HeartbeatReceived);
            assert_eq!(result, Err(TransitionError::AlreadyTerminal));
        }
    }

    #[test]
    fn invalid_event_for_state() {
        let result = transition(MembershipState::Initial, TransitionEvent::HeartbeatMissed);
        assert_eq!(result, Err(TransitionError::InvalidEventForState));
    }

    #[test]
    fn active_heartbeat_stays_active() {
        let result = transition(MembershipState::Active, TransitionEvent::HeartbeatReceived);
        assert_eq!(result, Ok(MembershipState::Active));
    }

    #[test]
    fn state_tracker_records_history() -> Result<(), TransitionError> {
        let mut tracker = MembershipStateTracker::new(42);
        assert_eq!(tracker.current(), MembershipState::Initial);

        let _ = tracker.try_transition(TransitionEvent::HeartbeatReceived, 0)?;
        assert_eq!(tracker.current(), MembershipState::Active);
        assert_eq!(tracker.history().len(), 1);

        let _ = tracker.try_transition(TransitionEvent::HeartbeatMissed, 1)?;
        assert_eq!(tracker.current(), MembershipState::Suspect);
        assert_eq!(tracker.history().len(), 2);
        Ok(())
    }

    #[test]
    fn state_tracker_transition_count() -> Result<(), TransitionError> {
        let mut tracker = MembershipStateTracker::new(42);
        let _ = tracker.try_transition(TransitionEvent::HeartbeatReceived, 0)?;
        let _ = tracker.try_transition(TransitionEvent::HeartbeatMissed, 1)?;
        let _ = tracker.try_transition(TransitionEvent::HeartbeatReceived, 2)?;

        assert_eq!(
            tracker.transition_count(TransitionEvent::HeartbeatReceived),
            2
        );
        assert_eq!(
            tracker.transition_count(TransitionEvent::HeartbeatMissed),
            1
        );
        Ok(())
    }
}
