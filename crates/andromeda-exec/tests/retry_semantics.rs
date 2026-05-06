//! Retry semantics and transient error classification tests.
//!
//! This integration test validates:
//! 1. Transient error classification (Transport, Resource, Protocol)
//! 2. Persistent error classification (Security, Contract, Catalog, etc.)
//! 3. Exponential backoff timing
//! 4. Max retries enforcement
//! 5. Audit logging of retry attempts
//! 6. Transparent retry to caller

use andromeda_core::{AndromedaErrorKind, InvocationId};
use andromeda_exec::retry::{ErrorRetryability, RetryDecision, RetryPolicy};
use andromeda_exec::traces::{AuditLedger, InMemoryAuditLedger};

// ============================================================================
// Tests: Error Classification
// ============================================================================

#[test]
fn test_error_classification_transient_transport() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Transport);
    assert_eq!(retryability, ErrorRetryability::Transient);
    assert!(retryability.is_transient());
}

#[test]
fn test_error_classification_transient_resource() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Resource);
    assert_eq!(retryability, ErrorRetryability::Transient);
}

#[test]
fn test_error_classification_transient_protocol() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Protocol);
    assert_eq!(retryability, ErrorRetryability::Transient);
}

#[test]
fn test_error_classification_persistent_security() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Security);
    assert_eq!(retryability, ErrorRetryability::Persistent);
    assert!(retryability.is_persistent());
}

#[test]
fn test_error_classification_persistent_contract() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Contract);
    assert_eq!(retryability, ErrorRetryability::Persistent);
}

#[test]
fn test_error_classification_persistent_srpl() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Srpl);
    assert_eq!(retryability, ErrorRetryability::Persistent);
}

#[test]
fn test_error_classification_persistent_catalog() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Catalog);
    assert_eq!(retryability, ErrorRetryability::Persistent);
}

#[test]
fn test_error_classification_persistent_execution() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Execution);
    assert_eq!(retryability, ErrorRetryability::Persistent);
}

#[test]
fn test_error_classification_persistent_storage() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Storage);
    assert_eq!(retryability, ErrorRetryability::Persistent);
}

#[test]
fn test_error_classification_persistent_transaction() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Transaction);
    assert_eq!(retryability, ErrorRetryability::Persistent);
}

// ============================================================================
// Tests: Retry Policy
// ============================================================================

#[test]
fn test_retry_policy_conservative_defaults() {
    let policy = RetryPolicy::conservative();
    assert_eq!(policy.initial_backoff_ms, 100);
    assert_eq!(policy.max_backoff_ms, 5_000);
    assert_eq!(policy.max_attempts, 8);
    assert!(policy.validate().is_ok());
}

#[test]
fn test_retry_policy_aggressive_defaults() {
    let policy = RetryPolicy::aggressive();
    assert_eq!(policy.initial_backoff_ms, 50);
    assert_eq!(policy.max_backoff_ms, 1_000);
    assert_eq!(policy.max_attempts, 3);
    assert!(policy.validate().is_ok());
}

#[test]
fn test_retry_policy_minimal_defaults() {
    let policy = RetryPolicy::minimal();
    assert_eq!(policy.initial_backoff_ms, 10);
    assert_eq!(policy.max_backoff_ms, 100);
    assert_eq!(policy.max_attempts, 2);
    assert!(policy.validate().is_ok());
}

#[test]
fn test_retry_policy_exponential_backoff_sequence() {
    let policy = RetryPolicy::conservative();
    assert_eq!(policy.delay_for_attempt(1).unwrap(), 100);
    assert_eq!(policy.delay_for_attempt(2).unwrap(), 200);
    assert_eq!(policy.delay_for_attempt(3).unwrap(), 400);
    assert_eq!(policy.delay_for_attempt(4).unwrap(), 800);
    assert_eq!(policy.delay_for_attempt(5).unwrap(), 1600);
    assert_eq!(policy.delay_for_attempt(6).unwrap(), 3200);
    assert_eq!(policy.delay_for_attempt(7).unwrap(), 5000); // capped
    assert_eq!(policy.delay_for_attempt(8).unwrap(), 5000); // capped
}

#[test]
fn test_retry_policy_backoff_saturation() {
    let policy = RetryPolicy::conservative();
    // All attempts >= 7 should cap at 5000ms
    assert_eq!(policy.delay_for_attempt(7).unwrap(), 5000);
    assert_eq!(policy.delay_for_attempt(8).unwrap(), 5000);
}

#[test]
fn test_retry_policy_decision_after_failure_retry() {
    let policy = RetryPolicy::conservative();
    let decision = policy.decision_after_failure(1).unwrap();
    assert!(decision.is_retry());
    match decision {
        RetryDecision::RetryAfter {
            next_attempt,
            delay_ms,
        } => {
            assert_eq!(next_attempt, 2);
            assert_eq!(delay_ms, 200);
        }
        _ => panic!("expected RetryAfter"),
    }
}

#[test]
fn test_retry_policy_decision_after_failure_give_up() {
    let policy = RetryPolicy::conservative();
    // After max_attempts (8), GiveUp is returned
    let decision = policy.decision_after_failure(8).unwrap();
    assert!(decision.is_give_up());
}

#[test]
fn test_retry_policy_validation_initial_zero() {
    let policy = RetryPolicy {
        initial_backoff_ms: 0,
        max_backoff_ms: 1000,
        max_attempts: 3,
        jitter_ppm: 0,
    };
    assert!(policy.validate().is_err());
}

#[test]
fn test_retry_policy_validation_max_less_initial() {
    let policy = RetryPolicy {
        initial_backoff_ms: 1000,
        max_backoff_ms: 500,
        max_attempts: 3,
        jitter_ppm: 0,
    };
    assert!(policy.validate().is_err());
}

#[test]
fn test_retry_policy_validation_max_attempts_zero() {
    let policy = RetryPolicy {
        initial_backoff_ms: 100,
        max_backoff_ms: 1000,
        max_attempts: 0,
        jitter_ppm: 0,
    };
    assert!(policy.validate().is_err());
}

#[test]
fn test_retry_policy_validation_max_attempts_exceeds_limit() {
    let policy = RetryPolicy {
        initial_backoff_ms: 100,
        max_backoff_ms: 1000,
        max_attempts: 64, // exceeds 32 limit
        jitter_ppm: 0,
    };
    assert!(policy.validate().is_err());
}

#[test]
fn test_retry_policy_validation_jitter_exceeds_limit() {
    let policy = RetryPolicy {
        initial_backoff_ms: 100,
        max_backoff_ms: 1000,
        max_attempts: 3,
        jitter_ppm: 2_000_000, // exceeds 1_000_000 limit
    };
    assert!(policy.validate().is_err());
}

// ============================================================================
// Tests: Audit Ledger Integration
// ============================================================================

#[test]
fn test_audit_ledger_append_and_retrieve() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id = andromeda_observe::TraceId::new(1);
    let invocation_id = InvocationId::new(1);

    let event = andromeda_exec::InvocationTraceEvent::ExecutionFailed {
        trace_id,
        invocation_id,
        failure_reason: "test failure".to_string(),
        recoverable: true,
    };

    assert!(ledger.append_trace(event.clone()).is_ok());
    assert_eq!(ledger.total_appended(), 1);
    assert_eq!(ledger.total_rejected(), 0);

    let snapshot = ledger.snapshot().unwrap();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0], event);
}

#[test]
fn test_audit_ledger_multiple_events() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id = andromeda_observe::TraceId::new(2);
    let invocation_id = InvocationId::new(2);

    for i in 0..5 {
        let event = andromeda_exec::InvocationTraceEvent::ExecutionFailed {
            trace_id,
            invocation_id,
            failure_reason: format!("failure {}", i),
            recoverable: true,
        };
        assert!(ledger.append_trace(event).is_ok());
    }

    assert_eq!(ledger.total_appended(), 5);
    let snapshot = ledger.snapshot().unwrap();
    assert_eq!(snapshot.len(), 5);
}

#[test]
fn test_audit_ledger_query_by_trace_id() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id_1 = andromeda_observe::TraceId::new(3);
    let trace_id_2 = andromeda_observe::TraceId::new(4);
    let invocation_id = InvocationId::new(3);

    let event1 = andromeda_exec::InvocationTraceEvent::ExecutionFailed {
        trace_id: trace_id_1,
        invocation_id,
        failure_reason: "error 1".to_string(),
        recoverable: true,
    };

    let event2 = andromeda_exec::InvocationTraceEvent::ExecutionFailed {
        trace_id: trace_id_2,
        invocation_id,
        failure_reason: "error 2".to_string(),
        recoverable: true,
    };

    assert!(ledger.append_trace(event1.clone()).is_ok());
    assert!(ledger.append_trace(event2.clone()).is_ok());

    let results = ledger.query_by_trace_id(trace_id_1).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], event1);
}

// ============================================================================
// Tests: Persistent Error Handling (Fail Immediately)
// ============================================================================

#[test]
fn test_persistent_error_security_fails_immediately() {
    // When a persistent Security error occurs, retry should not happen
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Security);
    assert_eq!(retryability, ErrorRetryability::Persistent);
    assert!(!retryability.is_transient());
}

#[test]
fn test_persistent_error_contract_fails_immediately() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Contract);
    assert_eq!(retryability, ErrorRetryability::Persistent);
}

#[test]
fn test_persistent_error_all_types() {
    let persistent_kinds = vec![
        AndromedaErrorKind::Security,
        AndromedaErrorKind::Contract,
        AndromedaErrorKind::Srpl,
        AndromedaErrorKind::Execution,
        AndromedaErrorKind::Catalog,
        AndromedaErrorKind::Storage,
        AndromedaErrorKind::Transaction,
        AndromedaErrorKind::Internal,
    ];

    for kind in persistent_kinds {
        let retryability = ErrorRetryability::classify(kind);
        assert_eq!(
            retryability,
            ErrorRetryability::Persistent,
            "expected {:?} to be persistent",
            kind
        );
    }
}

// ============================================================================
// Tests: Transient Error Handling (Retry with Backoff)
// ============================================================================

#[test]
fn test_transient_error_transport_is_retryable() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Transport);
    assert_eq!(retryability, ErrorRetryability::Transient);
    assert!(retryability.is_transient());
}

#[test]
fn test_transient_error_resource_is_retryable() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Resource);
    assert_eq!(retryability, ErrorRetryability::Transient);
}

#[test]
fn test_transient_error_protocol_is_retryable() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Protocol);
    assert_eq!(retryability, ErrorRetryability::Transient);
}

// ============================================================================
// Tests: Exponential Backoff Timing
// ============================================================================

#[test]
fn test_backoff_timing_100ms_initial() {
    let policy = RetryPolicy::conservative();
    let delay = policy.delay_for_attempt(1).unwrap();
    assert_eq!(delay, 100);
}

#[test]
fn test_backoff_timing_doubles_each_attempt() {
    let policy = RetryPolicy::conservative();
    let d1 = policy.delay_for_attempt(1).unwrap();
    let d2 = policy.delay_for_attempt(2).unwrap();
    let d3 = policy.delay_for_attempt(3).unwrap();

    assert_eq!(d2, d1 * 2);
    assert_eq!(d3, d2 * 2);
}

#[test]
fn test_backoff_timing_respects_max_cap() {
    let policy = RetryPolicy::conservative();
    // Attempts 7 and 8 should both be capped at 5000
    let d7 = policy.delay_for_attempt(7).unwrap();
    let d8 = policy.delay_for_attempt(8).unwrap();
    assert_eq!(d7, policy.max_backoff_ms);
    assert_eq!(d8, policy.max_backoff_ms);
}

#[test]
fn test_backoff_timing_cumulative_sequence() {
    let policy = RetryPolicy::conservative();
    let mut cumulative_ms = 0;
    let mut delays = Vec::new();

    for attempt in 1..=8 {
        let delay = policy.delay_for_attempt(attempt).unwrap();
        cumulative_ms += delay;
        delays.push(delay);
    }

    // Verify sequence
    assert_eq!(delays, vec![100, 200, 400, 800, 1600, 3200, 5000, 5000]);

    // Verify cumulative is within expected bounds
    // Total: 100 + 200 + 400 + 800 + 1600 + 3200 + 5000 + 5000 = 16300ms
    assert!(cumulative_ms >= 16_000 && cumulative_ms <= 17_000);
}

// ============================================================================
// Tests: Max Retries Enforcement
// ============================================================================

#[test]
fn test_max_retries_enforcement_attempt_1() {
    let policy = RetryPolicy::conservative();
    let decision = policy.decision_after_failure(1).unwrap();
    assert!(decision.is_retry());
}

#[test]
fn test_max_retries_enforcement_attempt_7() {
    let policy = RetryPolicy::conservative();
    // Attempt 7 should allow retry to attempt 8
    let decision = policy.decision_after_failure(7).unwrap();
    assert!(decision.is_retry());
}

#[test]
fn test_max_retries_enforcement_attempt_8_gives_up() {
    let policy = RetryPolicy::conservative();
    // After max_attempts (8), GiveUp
    let decision = policy.decision_after_failure(8).unwrap();
    assert!(decision.is_give_up());
}

#[test]
fn test_max_retries_enforcement_custom_policy() {
    let policy = RetryPolicy {
        initial_backoff_ms: 50,
        max_backoff_ms: 500,
        max_attempts: 3,
        jitter_ppm: 0,
    };

    // Attempt 1 → retry to 2
    assert!(policy.decision_after_failure(1).unwrap().is_retry());

    // Attempt 2 → retry to 3
    assert!(policy.decision_after_failure(2).unwrap().is_retry());

    // Attempt 3 → give up (at max)
    assert!(policy.decision_after_failure(3).unwrap().is_give_up());
}

// ============================================================================
// Tests: Retry Decision Tree
// ============================================================================

#[test]
fn test_retry_decision_after_first_failure() {
    let policy = RetryPolicy::conservative();
    let decision = policy.decision_after_failure(1).unwrap();

    match decision {
        RetryDecision::RetryAfter {
            next_attempt,
            delay_ms,
        } => {
            assert_eq!(next_attempt, 2);
            assert_eq!(delay_ms, 200); // 100 * 2^1
        }
        RetryDecision::GiveUp => {
            panic!("expected RetryAfter on first failure");
        }
    }
}

#[test]
fn test_retry_decision_after_fourth_failure() {
    let policy = RetryPolicy::conservative();
    let decision = policy.decision_after_failure(4).unwrap();

    match decision {
        RetryDecision::RetryAfter {
            next_attempt,
            delay_ms,
        } => {
            assert_eq!(next_attempt, 5);
            assert_eq!(delay_ms, 1600); // 100 * 2^4
        }
        RetryDecision::GiveUp => {
            panic!("expected RetryAfter on 4th failure");
        }
    }
}

#[test]
fn test_retry_decision_boundary_max_attempts() {
    let policy = RetryPolicy {
        initial_backoff_ms: 100,
        max_backoff_ms: 1000,
        max_attempts: 3,
        jitter_ppm: 0,
    };

    let d1 = policy.decision_after_failure(1).unwrap();
    assert!(d1.is_retry());

    let d2 = policy.decision_after_failure(2).unwrap();
    assert!(d2.is_retry());

    let d3 = policy.decision_after_failure(3).unwrap();
    assert!(d3.is_give_up());
}

// ============================================================================
// Tests: Integration Scenarios
// ============================================================================

#[test]
fn test_scenario_persistent_error_fails_immediately() {
    // Persistent error should not trigger retry
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Security);
    assert_eq!(retryability, ErrorRetryability::Persistent);
}

#[test]
fn test_scenario_max_retries_exhausted() {
    let policy = RetryPolicy::conservative();

    // Simulate retry exhaustion at max_attempts
    for attempt in 1..8 {
        let decision = policy.decision_after_failure(attempt).unwrap();
        assert!(
            decision.is_retry(),
            "attempt {} should allow retry",
            attempt
        );
    }

    // After max, GiveUp
    let decision = policy.decision_after_failure(8).unwrap();
    assert!(decision.is_give_up());
}

#[test]
fn test_scenario_aggressive_policy_fewer_attempts() {
    let policy = RetryPolicy::aggressive();

    // Aggressive only allows 3 attempts
    assert_eq!(policy.max_attempts, 3);

    let d1 = policy.decision_after_failure(1).unwrap();
    assert!(d1.is_retry());

    let d2 = policy.decision_after_failure(2).unwrap();
    assert!(d2.is_retry());

    let d3 = policy.decision_after_failure(3).unwrap();
    assert!(d3.is_give_up());
}

#[test]
fn test_scenario_minimal_policy_fast_fail() {
    let policy = RetryPolicy::minimal();

    // Minimal only allows 2 attempts
    assert_eq!(policy.max_attempts, 2);

    let d1 = policy.decision_after_failure(1).unwrap();
    assert!(d1.is_retry());

    let d2 = policy.decision_after_failure(2).unwrap();
    assert!(d2.is_give_up());
}

#[test]
fn test_scenario_audit_trace_consistency() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id = andromeda_observe::TraceId::new(5);
    let invocation_id = InvocationId::new(4);

    // Simulate 3 retry attempts with same trace_id
    for attempt in 0..3 {
        let event = andromeda_exec::InvocationTraceEvent::ExecutionFailed {
            trace_id,
            invocation_id,
            failure_reason: format!("transient error attempt {}", attempt),
            recoverable: true,
        };
        assert!(ledger.append_trace(event).is_ok());
    }

    // All 3 should be queryable by trace_id
    let results = ledger.query_by_trace_id(trace_id).unwrap();
    assert_eq!(results.len(), 3);

    // All should share the same trace_id and invocation_id
    for event in results {
        assert_eq!(event.trace_id(), trace_id);
    }
}
