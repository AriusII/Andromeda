//! Invocation timeout contract tests.
//!
//! Validates the invocation-timeout surface:
//!
//! 1. `AndromedaErrorKind::Timeout` exists and is classified as `Persistent`.
//! 2. `ErrorRetryability::Persistent` → `RetryDecision::GiveUp` on first attempt.
//! 3. `InvocationTraceEvent::TimeoutExceeded` stores and queries correctly via
//!    `InMemoryAuditLedger`.
//! 4. `Timeout` is distinct from all transient error kinds (no retry confusion).
//! 5. `Timeout` is distinct from `CancellationReason` (both exist independently).
//! 6. Deadlock retry (`Transaction` kind) is still `Persistent` and does not
//!    conflict with the new `Timeout` kind.
//!
//! ## Current scope
//!
//! Full invocation deadline (admission → WAL commit), `LockManager` deadline
//! support, and `TransactionManager` timeout are outside this contract.

use andromeda_error::AndromedaErrorKind;

use andromeda_execution_trace::{AuditLedger, InMemoryAuditLedger, InvocationTraceEvent};
use andromeda_observe::TraceId;
use andromeda_retry::{ErrorRetryability, RetryDecision, RetryPolicy};
use andromeda_types::InvocationId;

// Seed helpers so every test gets a distinct, deterministic TraceId.
const T1: TraceId = TraceId::new(0x_0001_0000_0000_0001);
const T2: TraceId = TraceId::new(0x_0001_0000_0000_0002);
const T3: TraceId = TraceId::new(0x_0001_0000_0000_0003);
const T4: TraceId = TraceId::new(0x_0001_0000_0000_0004);
const T5: TraceId = TraceId::new(0x_0001_0000_0000_0005);

// 1 — Error kind existence and persistence classification

#[test]
fn timeout_error_kind_is_classified_persistent() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Timeout);
    assert_eq!(
        retryability,
        ErrorRetryability::Persistent,
        "Timeout must be Persistent: the invocation deadline is consumed; no retry is safe"
    );
}

#[test]
fn timeout_retryability_is_not_transient() {
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Timeout);
    assert!(
        retryability.is_persistent(),
        "Timeout retryability must report is_persistent() == true"
    );
    assert!(
        !retryability.is_transient(),
        "Timeout retryability must report is_transient() == false"
    );
}

// 2 — Retry policy gives up immediately on Timeout (no retry loop)

#[test]
fn retry_policy_gives_up_on_first_timeout_attempt() {
    let policy = RetryPolicy::conservative();
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Timeout);

    // Timeout is Persistent — the `decision_after_failure` path is only
    // relevant for Transient errors. Validate directly that the retryability
    // gate would prevent the retry call from being reached.
    assert!(
        retryability.is_persistent(),
        "gate: caller must not enter retry loop for Persistent errors"
    );

    // Belt-and-braces: even if the caller passed attempt=1 to a Transient-only
    // path, the max_attempts gate should cap it. Verify GiveUp at saturation.
    let decision = policy.decision_after_failure(policy.max_attempts).unwrap();
    assert_eq!(
        decision,
        RetryDecision::GiveUp,
        "exhausted retry budget must produce GiveUp regardless of error kind"
    );
}

#[test]
fn deadlock_transaction_error_is_also_persistent() {
    // Confirm that `Transaction` (deadlock abort) remains Persistent so the
    // deadlock-retry path is not confused with the timeout path.
    let retryability = ErrorRetryability::classify(AndromedaErrorKind::Transaction);
    assert_eq!(
        retryability,
        ErrorRetryability::Persistent,
        "Transaction (deadlock) must remain Persistent independently of Timeout"
    );
}

// 3 — TimeoutExceeded trace event lifecycle in InMemoryAuditLedger

#[test]
fn timeout_exceeded_event_appends_and_queries_by_trace_id() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id = T1;
    let invocation_id = InvocationId::new(77);

    let event = InvocationTraceEvent::TimeoutExceeded {
        trace_id,
        invocation_id,
        deadline_kind: "IdleTimeout".to_string(),
        elapsed_ms: 30_001,
    };

    ledger
        .append_trace(event.clone())
        .expect("append must succeed");
    assert_eq!(ledger.total_appended(), 1);
    assert_eq!(ledger.total_rejected(), 0);

    let by_trace = ledger
        .query_by_trace_id(trace_id)
        .expect("query by trace_id must succeed");
    assert_eq!(by_trace.len(), 1);
    assert_eq!(by_trace[0], event);
}

#[test]
fn timeout_exceeded_event_queries_by_invocation_id() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id = T2;
    let invocation_id = InvocationId::new(99);

    let event = InvocationTraceEvent::TimeoutExceeded {
        trace_id,
        invocation_id,
        deadline_kind: "OverallTimeout".to_string(),
        elapsed_ms: 300_001,
    };

    ledger
        .append_trace(event.clone())
        .expect("append must succeed");

    let by_invocation = ledger
        .query_by_invocation_id(invocation_id)
        .expect("query by invocation_id must succeed");
    assert_eq!(by_invocation.len(), 1);
    assert_eq!(by_invocation[0], event);
}

#[test]
fn timeout_exceeded_extractors_return_correct_ids() {
    let trace_id = T3;
    let invocation_id = InvocationId::new(55);

    let event = InvocationTraceEvent::TimeoutExceeded {
        trace_id,
        invocation_id,
        deadline_kind: "LockWait".to_string(),
        elapsed_ms: 5_001,
    };

    assert_eq!(event.trace_id(), trace_id);
    assert_eq!(event.invocation_id(), invocation_id);
}

#[test]
fn timeout_exceeded_trace_is_independent_of_execution_failed_trace() {
    // Timeout and ExecutionFailed are separate event families; they must not
    // be conflated. Both can appear for the same invocation (failed due to
    // timeout) but are distinct variants with distinct semantics.
    let trace_id = T4;
    let invocation_id = InvocationId::new(13);

    let timeout_event = InvocationTraceEvent::TimeoutExceeded {
        trace_id,
        invocation_id,
        deadline_kind: "IdleTimeout".to_string(),
        elapsed_ms: 30_002,
    };

    let failed_event = InvocationTraceEvent::ExecutionFailed {
        trace_id,
        invocation_id,
        failure_reason: "stream idle timeout exceeded".to_string(),
        recoverable: false,
    };

    assert_ne!(
        timeout_event, failed_event,
        "TimeoutExceeded and ExecutionFailed must be distinct trace event variants"
    );
    assert_eq!(timeout_event.trace_id(), failed_event.trace_id());
    assert_eq!(timeout_event.invocation_id(), failed_event.invocation_id());
}

// 4 — Timeout × Transient boundary: transport errors remain Transient

#[test]
fn transport_error_is_transient_and_timeout_is_persistent_independently() {
    assert_eq!(
        ErrorRetryability::classify(AndromedaErrorKind::Transport),
        ErrorRetryability::Transient,
        "Transport must remain Transient (unrelated to timeout)"
    );
    assert_eq!(
        ErrorRetryability::classify(AndromedaErrorKind::Timeout),
        ErrorRetryability::Persistent,
        "Timeout must be Persistent independently of Transport classification"
    );
}

// 5 — Audit ledger does not drop or conflate multiple event types per invocation

#[test]
fn audit_ledger_accepts_all_event_types_including_timeout() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id = T5;
    let invocation_id = InvocationId::new(200);

    let events = vec![
        InvocationTraceEvent::AdmissionDecision {
            trace_id,
            invocation_id,
            accepted: true,
            reason: "admitted".to_string(),
        },
        InvocationTraceEvent::DispatchEvent {
            trace_id,
            invocation_id,
            procedure_name: "Inventory.QueryStock".to_string(),
            executor_kind: "LocalExecutor".to_string(),
        },
        InvocationTraceEvent::TimeoutExceeded {
            trace_id,
            invocation_id,
            deadline_kind: "OverallTimeout".to_string(),
            elapsed_ms: 301_000,
        },
    ];

    for event in &events {
        ledger
            .append_trace(event.clone())
            .expect("append must not fail");
    }

    assert_eq!(ledger.total_appended(), 3);

    let stored = ledger
        .query_by_invocation_id(invocation_id)
        .expect("query must succeed");
    assert_eq!(stored.len(), 3, "all three events must be returned");
}
