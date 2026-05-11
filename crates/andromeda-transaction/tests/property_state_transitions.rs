//! Property-style tests for TransactionStateMachine state transitions.
//!
//! Uses manual enumeration of all (state, event) combinations to prove:
//! - Every valid transition is well-defined
//! - Every invalid transition returns an error
//! - Poisoned state only allows RollbackRequested
//! - Disposed state rejects all events
//! - DurableWalFlushed only works after mark_durable_commit_lsn is set

use andromeda_transaction::{TransactionEvent, TransactionState, TransactionStateMachine};
use andromeda_types::TransactionId;
use std::collections::HashSet;

fn all_states() -> &'static [TransactionState] {
    &[
        TransactionState::Created,
        TransactionState::Active,
        TransactionState::Committing,
        TransactionState::Committed,
        TransactionState::Failed,
        TransactionState::Poisoned,
        TransactionState::RollingBack,
        TransactionState::RolledBack,
        TransactionState::Disposed,
    ]
}

fn all_events() -> &'static [TransactionEvent] {
    &[
        TransactionEvent::Begin,
        TransactionEvent::CommitRequested,
        TransactionEvent::DurableWalFlushed,
        TransactionEvent::Fail,
        TransactionEvent::Poison,
        TransactionEvent::RollbackRequested,
        TransactionEvent::RollbackComplete,
        TransactionEvent::Dispose,
    ]
}

// The valid (state, event) -> next_state transitions per state.rs
fn valid_transitions() -> &'static [(TransactionState, TransactionEvent, TransactionState)] {
    &[
        (
            TransactionState::Created,
            TransactionEvent::Begin,
            TransactionState::Active,
        ),
        (
            TransactionState::Active,
            TransactionEvent::CommitRequested,
            TransactionState::Committing,
        ),
        (
            TransactionState::Committing,
            TransactionEvent::DurableWalFlushed,
            TransactionState::Committed,
        ),
        (
            TransactionState::Committed,
            TransactionEvent::Dispose,
            TransactionState::Disposed,
        ),
        (
            TransactionState::Active,
            TransactionEvent::RollbackRequested,
            TransactionState::RollingBack,
        ),
        (
            TransactionState::Active,
            TransactionEvent::Fail,
            TransactionState::Failed,
        ),
        (
            TransactionState::Failed,
            TransactionEvent::RollbackRequested,
            TransactionState::RollingBack,
        ),
        (
            TransactionState::Active,
            TransactionEvent::Poison,
            TransactionState::Poisoned,
        ),
        (
            TransactionState::Poisoned,
            TransactionEvent::RollbackRequested,
            TransactionState::RollingBack,
        ),
        (
            TransactionState::RollingBack,
            TransactionEvent::RollbackComplete,
            TransactionState::RolledBack,
        ),
        (
            TransactionState::RolledBack,
            TransactionEvent::Dispose,
            TransactionState::Disposed,
        ),
    ]
}

#[test]
fn all_valid_state_transitions_produce_expected_next_state() {
    for (from, event, expected_next) in valid_transitions() {
        let result = from.apply(*event);
        assert!(
            result.is_ok(),
            "Valid transition {:?} --{:?}--> {:?} must not error, got: {:?}",
            from,
            event,
            expected_next,
            result
        );
        assert_eq!(
            result.unwrap(),
            *expected_next,
            "Valid transition {:?} --{:?}--> must produce {:?}",
            from,
            event,
            expected_next
        );
    }
}

#[test]
fn invalid_state_transitions_are_exhaustively_rejected() {
    // Build set of valid (state, event) pairs (from TransactionState::apply)
    let valid_pairs: HashSet<(u8, u8)> = valid_transitions()
        .iter()
        .map(|(s, e, _)| (state_index(s), event_index(e)))
        .collect();

    for state in all_states() {
        for event in all_events() {
            let key = (state_index(state), event_index(event));
            if !valid_pairs.contains(&key) {
                let result = state.apply(*event);
                assert!(
                    result.is_err(),
                    "Invalid transition {:?} --{:?}--> must return error",
                    state,
                    event
                );
            }
        }
    }
}

#[test]
fn poisoned_state_rejects_all_events_except_rollback_requested() {
    for event in all_events() {
        if *event == TransactionEvent::RollbackRequested {
            let result = TransactionState::Poisoned.apply(*event);
            assert!(result.is_ok(), "Poisoned + RollbackRequested must be valid");
            assert_eq!(result.unwrap(), TransactionState::RollingBack);
        } else {
            let result = TransactionState::Poisoned.apply(*event);
            assert!(
                result.is_err(),
                "Poisoned + {:?} must be rejected (only RollbackRequested is allowed)",
                event
            );
        }
    }
}

#[test]
fn disposed_state_rejects_every_event() {
    for event in all_events() {
        let result = TransactionState::Disposed.apply(*event);
        assert!(result.is_err(), "Disposed + {:?} must be rejected", event);
    }
}

#[test]
fn committed_state_rejects_every_event_except_dispose() {
    for event in all_events() {
        if *event == TransactionEvent::Dispose {
            continue; // valid
        }
        let result = TransactionState::Committed.apply(*event);
        assert!(
            result.is_err(),
            "Committed + {:?} must be rejected (only Dispose allowed)",
            event
        );
    }
}

#[test]
fn rolled_back_state_rejects_every_event_except_dispose() {
    for event in all_events() {
        if *event == TransactionEvent::Dispose {
            continue; // valid
        }
        let result = TransactionState::RolledBack.apply(*event);
        assert!(
            result.is_err(),
            "RolledBack + {:?} must be rejected (only Dispose allowed)",
            event
        );
    }
}

#[test]
fn state_machine_commit_requires_durable_lsn_at_every_attempt() {
    // Prove that DurableWalFlushed on Committing state is always rejected
    // without prior set of durable_commit_lsn
    let mut tx = TransactionStateMachine::new(TransactionId::new(1));
    tx.begin().unwrap();
    tx.request_commit().unwrap();

    // Applying DurableWalFlushed directly without marking durable LSN must fail
    let err = tx.apply(TransactionEvent::DurableWalFlushed);
    assert!(
        err.is_err(),
        "DurableWalFlushed without durable LSN must fail"
    );
}

#[test]
fn state_machine_rollback_requires_durable_lsn_at_every_attempt() {
    // Prove that RollbackComplete on RollingBack state is always rejected
    // without prior set of durable_rollback_lsn
    let mut tx = TransactionStateMachine::new(TransactionId::new(2));
    tx.begin().unwrap();
    tx.request_rollback().unwrap();

    // Applying RollbackComplete directly without marking durable LSN must fail
    let err = tx.apply(TransactionEvent::RollbackComplete);
    assert!(
        err.is_err(),
        "RollbackComplete without durable LSN must fail"
    );
}

#[test]
fn terminal_states_is_terminal_invariant() {
    for state in all_states() {
        let expected = matches!(
            state,
            TransactionState::Committed | TransactionState::RolledBack | TransactionState::Disposed
        );
        assert_eq!(
            state.is_terminal(),
            expected,
            "{:?} is_terminal() must be {:?}",
            state,
            expected
        );
    }
}

#[test]
fn state_machine_fail_then_rollback_path() {
    // Active -> Failed -> RollingBack -> RolledBack
    let mut tx = TransactionStateMachine::new(TransactionId::new(7));
    tx.begin().unwrap();
    tx.apply(TransactionEvent::Fail).unwrap();
    assert_eq!(tx.state(), TransactionState::Failed);
    tx.request_rollback().unwrap();
    tx.complete_rollback_after_durable_flush(42).unwrap();
    assert_eq!(tx.state(), TransactionState::RolledBack);
    assert!(tx.is_durable_rolled_back());
}

#[test]
fn state_machine_poison_then_rollback_path() {
    // Active -> Poisoned -> RollingBack -> RolledBack
    let mut tx = TransactionStateMachine::new(TransactionId::new(8));
    tx.begin().unwrap();
    tx.apply(TransactionEvent::Poison).unwrap();
    assert_eq!(tx.state(), TransactionState::Poisoned);
    tx.request_rollback().unwrap();
    tx.complete_rollback_after_durable_flush(99).unwrap();
    assert_eq!(tx.state(), TransactionState::RolledBack);
    assert!(tx.is_durable_rolled_back());
}

#[test]
fn created_state_only_allows_begin() {
    for event in all_events() {
        if *event == TransactionEvent::Begin {
            continue; // valid
        }
        let result = TransactionState::Created.apply(*event);
        assert!(
            result.is_err(),
            "Created + {:?} must be rejected (only Begin allowed)",
            event
        );
    }
}

#[test]
fn committing_state_only_allows_durable_wal_flushed() {
    for event in all_events() {
        if *event == TransactionEvent::DurableWalFlushed {
            continue; // valid at TransactionState level
        }
        let result = TransactionState::Committing.apply(*event);
        assert!(
            result.is_err(),
            "Committing + {:?} must be rejected (only DurableWalFlushed allowed)",
            event
        );
    }
}

#[test]
fn rolling_back_state_only_allows_rollback_complete() {
    for event in all_events() {
        if *event == TransactionEvent::RollbackComplete {
            continue; // valid at TransactionState level
        }
        let result = TransactionState::RollingBack.apply(*event);
        assert!(
            result.is_err(),
            "RollingBack + {:?} must be rejected (only RollbackComplete allowed)",
            event
        );
    }
}

#[test]
fn failed_state_only_allows_rollback_requested() {
    for event in all_events() {
        if *event == TransactionEvent::RollbackRequested {
            continue; // valid
        }
        let result = TransactionState::Failed.apply(*event);
        assert!(
            result.is_err(),
            "Failed + {:?} must be rejected (only RollbackRequested allowed)",
            event
        );
    }
}

#[test]
fn active_state_allows_multiple_transitions() {
    // Active can transition to: Committing, RollingBack, Failed, Poisoned
    let valid_from_active = [
        TransactionEvent::CommitRequested,
        TransactionEvent::RollbackRequested,
        TransactionEvent::Fail,
        TransactionEvent::Poison,
    ];

    for event in all_events() {
        let result = TransactionState::Active.apply(*event);
        if valid_from_active.contains(event) {
            assert!(result.is_ok(), "Active + {:?} must be valid", event);
        } else {
            assert!(result.is_err(), "Active + {:?} must be invalid", event);
        }
    }
}

// Helper: map state to index for HashSet key
fn state_index(s: &TransactionState) -> u8 {
    match s {
        TransactionState::Created => 0,
        TransactionState::Active => 1,
        TransactionState::Committing => 2,
        TransactionState::Committed => 3,
        TransactionState::Failed => 4,
        TransactionState::Poisoned => 5,
        TransactionState::RollingBack => 6,
        TransactionState::RolledBack => 7,
        TransactionState::Disposed => 8,
    }
}

fn event_index(e: &TransactionEvent) -> u8 {
    match e {
        TransactionEvent::Begin => 0,
        TransactionEvent::CommitRequested => 1,
        TransactionEvent::DurableWalFlushed => 2,
        TransactionEvent::Fail => 3,
        TransactionEvent::Poison => 4,
        TransactionEvent::RollbackRequested => 5,
        TransactionEvent::RollbackComplete => 6,
        TransactionEvent::Dispose => 7,
    }
}
