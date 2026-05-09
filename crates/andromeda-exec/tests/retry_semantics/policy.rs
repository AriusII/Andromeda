use crate::support::{assert_retry_after, custom_policy};
use andromeda_retry::{RetryDecision, RetryPolicy};

#[test]
fn test_retry_policy_defaults_are_valid() {
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
fn test_retry_policy_exponential_backoff_sequence() {
    let policy = RetryPolicy::conservative();
    let delays: Vec<_> = (1..=8)
        .map(|attempt| policy.delay_for_attempt(attempt).unwrap())
        .collect();

    assert_eq!(delays, vec![100, 200, 400, 800, 1600, 3200, 5000, 5000]);
    assert_eq!(policy.delay_for_attempt(7).unwrap(), 5000);
    assert_eq!(policy.delay_for_attempt(8).unwrap(), 5000);
}

#[test]
fn test_retry_policy_backoff_timing_contract() {
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
fn test_retry_policy_backoff_cumulative_sequence() {
    let policy = RetryPolicy::conservative();
    let delays: Vec<_> = (1..=8)
        .map(|attempt| policy.delay_for_attempt(attempt).unwrap())
        .collect();
    let cumulative_ms: u64 = delays.iter().sum();

    assert_eq!(delays, vec![100, 200, 400, 800, 1600, 3200, 5000, 5000]);
    assert!((16_000..=17_000).contains(&cumulative_ms));
}

#[test]
fn test_retry_policy_decision_after_failure() {
    let policy = RetryPolicy::conservative();

    assert_retry_after(policy.decision_after_failure(1).unwrap(), 2, 200);
    assert_retry_after(policy.decision_after_failure(4).unwrap(), 5, 1600);
    assert!(policy.decision_after_failure(8).unwrap().is_give_up());
}

#[test]
fn test_retry_policy_validation_rejects_invalid_bounds() {
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
fn test_max_retries_enforcement() {
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
fn test_retry_decision_boundary_max_attempts() {
    let policy = custom_policy(100, 1000, 3, 0);

    assert!(policy.decision_after_failure(1).unwrap().is_retry());
    assert!(policy.decision_after_failure(2).unwrap().is_retry());
    assert!(policy.decision_after_failure(3).unwrap().is_give_up());
}

#[test]
fn test_retry_decision_shape() {
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
