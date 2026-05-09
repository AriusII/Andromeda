use andromeda_error::AndromedaErrorKind;
use andromeda_retry::{ErrorRetryability, RetryDecision, RetryPolicy};

const TRANSIENT_KINDS: [AndromedaErrorKind; 3] = [
    AndromedaErrorKind::Transport,
    AndromedaErrorKind::Resource,
    AndromedaErrorKind::Protocol,
];

const PERSISTENT_KINDS: [AndromedaErrorKind; 9] = [
    AndromedaErrorKind::Security,
    AndromedaErrorKind::Contract,
    AndromedaErrorKind::Srpl,
    AndromedaErrorKind::Execution,
    AndromedaErrorKind::Catalog,
    AndromedaErrorKind::Storage,
    AndromedaErrorKind::Transaction,
    AndromedaErrorKind::Internal,
    AndromedaErrorKind::Timeout,
];

const fn custom_policy(
    initial_backoff_ms: u64,
    max_backoff_ms: u64,
    max_attempts: u32,
    jitter_ppm: u32,
) -> RetryPolicy {
    RetryPolicy {
        initial_backoff_ms,
        max_backoff_ms,
        max_attempts,
        jitter_ppm,
    }
}

fn assert_retry_after(decision: RetryDecision, next_attempt: u32, delay_ms: u64) {
    assert!(decision.is_retry());
    match decision {
        RetryDecision::RetryAfter {
            next_attempt: actual_attempt,
            delay_ms: actual_delay,
        } => {
            assert_eq!(actual_attempt, next_attempt);
            assert_eq!(actual_delay, delay_ms);
        },
        RetryDecision::GiveUp => panic!("expected RetryAfter"),
    }
}

#[test]
fn error_classification_transient_types() {
    for kind in TRANSIENT_KINDS {
        let retryability = ErrorRetryability::classify(kind);
        assert_eq!(
            retryability,
            ErrorRetryability::Transient,
            "expected {:?} to be transient",
            kind
        );
        assert!(retryability.is_transient());
    }
}

#[test]
fn error_classification_persistent_types() {
    for kind in PERSISTENT_KINDS {
        let retryability = ErrorRetryability::classify(kind);
        assert_eq!(
            retryability,
            ErrorRetryability::Persistent,
            "expected {:?} to be persistent",
            kind
        );
        assert!(retryability.is_persistent());
    }
}

#[test]
fn security_contract_and_timeout_errors_fail_immediately() {
    for kind in [
        AndromedaErrorKind::Security,
        AndromedaErrorKind::Contract,
        AndromedaErrorKind::Timeout,
    ] {
        let retryability = ErrorRetryability::classify(kind);
        assert_eq!(retryability, ErrorRetryability::Persistent);
        assert!(!retryability.is_transient());
    }
}

#[test]
fn transport_resource_and_protocol_errors_are_retryable() {
    for kind in TRANSIENT_KINDS {
        let retryability = ErrorRetryability::classify(kind);
        assert_eq!(retryability, ErrorRetryability::Transient);
        assert!(retryability.is_transient());
    }
}

#[test]
fn retry_policy_defaults_are_valid() {
    for (policy, initial, max, attempts) in [
        (RetryPolicy::conservative(), 100, 5_000, 8),
        (RetryPolicy::aggressive(), 50, 1_000, 3),
        (RetryPolicy::minimal(), 10, 100, 2),
    ] {
        assert_eq!(policy.initial_backoff_ms, initial);
        assert_eq!(policy.max_backoff_ms, max);
        assert_eq!(policy.max_attempts, attempts);
        assert!(policy.validate().is_ok());
    }
}

#[test]
fn retry_policy_exponential_backoff_sequence() {
    let policy = RetryPolicy::conservative();
    let delays: Vec<_> = (1..=8)
        .map(|attempt| policy.delay_for_attempt(attempt).unwrap())
        .collect();

    assert_eq!(delays, vec![100, 200, 400, 800, 1600, 3200, 5000, 5000]);
    assert_eq!(policy.delay_for_attempt(7).unwrap(), 5000);
    assert_eq!(policy.delay_for_attempt(8).unwrap(), 5000);
}

#[test]
fn retry_policy_backoff_timing_contract() {
    let policy = RetryPolicy::conservative();
    let d1 = policy.delay_for_attempt(1).unwrap();
    let d2 = policy.delay_for_attempt(2).unwrap();
    let d3 = policy.delay_for_attempt(3).unwrap();
    let d7 = policy.delay_for_attempt(7).unwrap();
    let d8 = policy.delay_for_attempt(8).unwrap();

    assert_eq!(d1, 100);
    assert_eq!(d2, d1 * 2);
    assert_eq!(d3, d2 * 2);
    assert_eq!(d7, policy.max_backoff_ms);
    assert_eq!(d8, policy.max_backoff_ms);
}

#[test]
fn retry_policy_backoff_cumulative_sequence() {
    let policy = RetryPolicy::conservative();
    let delays: Vec<_> = (1..=8)
        .map(|attempt| policy.delay_for_attempt(attempt).unwrap())
        .collect();
    let cumulative_ms: u64 = delays.iter().sum();

    assert_eq!(delays, vec![100, 200, 400, 800, 1600, 3200, 5000, 5000]);
    assert!((16_000..=17_000).contains(&cumulative_ms));
}

#[test]
fn retry_policy_decision_after_failure() {
    let policy = RetryPolicy::conservative();

    assert_retry_after(policy.decision_after_failure(1).unwrap(), 2, 200);
    assert_retry_after(policy.decision_after_failure(4).unwrap(), 5, 1600);
    assert!(policy.decision_after_failure(8).unwrap().is_give_up());
}

#[test]
fn retry_policy_validation_rejects_invalid_bounds() {
    for policy in [
        custom_policy(0, 1000, 3, 0),
        custom_policy(1000, 500, 3, 0),
        custom_policy(100, 1000, 0, 0),
        custom_policy(100, 1000, 64, 0),
        custom_policy(100, 1000, 3, 2_000_000),
    ] {
        assert!(policy.validate().is_err());
    }
}

#[test]
fn max_retries_enforcement() {
    let policy = RetryPolicy::conservative();
    assert!(policy.decision_after_failure(1).unwrap().is_retry());
    assert!(policy.decision_after_failure(7).unwrap().is_retry());
    assert!(policy.decision_after_failure(8).unwrap().is_give_up());

    let custom = custom_policy(50, 500, 3, 0);
    assert!(custom.decision_after_failure(1).unwrap().is_retry());
    assert!(custom.decision_after_failure(2).unwrap().is_retry());
    assert!(custom.decision_after_failure(3).unwrap().is_give_up());
}

#[test]
fn retry_decision_boundary_max_attempts() {
    let policy = custom_policy(100, 1000, 3, 0);

    assert!(policy.decision_after_failure(1).unwrap().is_retry());
    assert!(policy.decision_after_failure(2).unwrap().is_retry());
    assert!(policy.decision_after_failure(3).unwrap().is_give_up());
}

#[test]
fn retry_decision_shape() {
    assert_retry_after(
        RetryDecision::RetryAfter {
            next_attempt: 2,
            delay_ms: 100,
        },
        2,
        100,
    );
    assert!(RetryDecision::GiveUp.is_give_up());
}
