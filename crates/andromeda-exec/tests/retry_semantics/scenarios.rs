use crate::support::execution_failed;
use andromeda_error::AndromedaErrorKind;
use andromeda_execution_trace::{AuditLedger, InMemoryAuditLedger};
use andromeda_observe::TraceId;
use andromeda_retry::{ErrorRetryability, RetryPolicy};
use andromeda_types::InvocationId;

#[test]
fn test_scenario_persistent_error_fails_immediately() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Security);
    assert_eq!(retryability, ErrorRetryability::Persistent);
}

#[test]
fn test_scenario_max_retries_exhausted() {
    let policy = RetryPolicy::conservative();

    for attempt in 1..8 {
        let decision = policy.decision_after_failure(attempt).unwrap();
        assert!(
            decision.is_retry(),
            "attempt {} should allow retry",
            attempt
        );
    }

    let decision = policy.decision_after_failure(8).unwrap();
    assert!(decision.is_give_up());
}

#[test]
fn test_scenario_aggressive_policy_fewer_attempts() {
    let policy = RetryPolicy::aggressive();
    assert_eq!(policy.max_attempts, 3);
    assert!(policy.decision_after_failure(1).unwrap().is_retry());
    assert!(policy.decision_after_failure(2).unwrap().is_retry());
    assert!(policy.decision_after_failure(3).unwrap().is_give_up());
}

#[test]
fn test_scenario_minimal_policy_fast_fail() {
    let policy = RetryPolicy::minimal();
    assert_eq!(policy.max_attempts, 2);
    assert!(policy.decision_after_failure(1).unwrap().is_retry());
    assert!(policy.decision_after_failure(2).unwrap().is_give_up());
}

#[test]
fn test_scenario_audit_trace_consistency() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id = TraceId::new(5);
    let invocation_id = InvocationId::new(4);

    for attempt in 0..3 {
        assert!(
            ledger
                .append_trace(execution_failed(
                    trace_id,
                    invocation_id,
                    format!("transient error attempt {}", attempt),
                ))
                .is_ok()
        );
    }

    let results = ledger.query_by_trace_id(trace_id).unwrap();
    assert_eq!(results.len(), 3);

    for event in results {
        assert_eq!(event.trace_id(), trace_id);
        assert_eq!(event.invocation_id(), invocation_id);
    }
}
