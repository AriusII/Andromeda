use andromeda_error::AndromedaErrorKind;
use andromeda_execution_trace::InvocationTraceEvent;
use andromeda_observe::TraceId;
use andromeda_retry::{RetryDecision, RetryPolicy};
use andromeda_types::InvocationId;

pub const TRANSIENT_KINDS: [AndromedaErrorKind; 3] = [
    AndromedaErrorKind::Transport,
    AndromedaErrorKind::Resource,
    AndromedaErrorKind::Protocol,
];

pub const PERSISTENT_KINDS: [AndromedaErrorKind; 8] = [
    AndromedaErrorKind::Security,
    AndromedaErrorKind::Contract,
    AndromedaErrorKind::Srpl,
    AndromedaErrorKind::Execution,
    AndromedaErrorKind::Catalog,
    AndromedaErrorKind::Storage,
    AndromedaErrorKind::Transaction,
    AndromedaErrorKind::Internal,
];

pub const fn custom_policy(
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

pub fn execution_failed(
    trace_id: TraceId,
    invocation_id: InvocationId,
    failure_reason: impl Into<String>,
) -> InvocationTraceEvent {
    InvocationTraceEvent::ExecutionFailed {
        trace_id,
        invocation_id,
        failure_reason: failure_reason.into(),
        recoverable: true,
    }
}

pub fn assert_retry_after(decision: RetryDecision, next_attempt: u32, delay_ms: u64) {
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
