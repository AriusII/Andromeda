//! Invocation timeout trace contract tests.
//!
//! Validates the invocation-timeout trace surface:
//!
//! 1. `InvocationTraceEvent::TimeoutExceeded` stores and queries correctly via
//!    `InMemoryAuditLedger`.
//! 2. `TimeoutExceeded` stays distinct from `ExecutionFailed`.
//! 3. Timeout events can coexist with admission and dispatch events for the
//!    same invocation without silent drops.
//!
//! ## Current scope
//!
//! Full invocation deadline (admission → WAL commit), `LockManager` deadline
//! support, and `TransactionManager` timeout are outside this contract.

use andromeda_execution_trace::{AuditLedger, InMemoryAuditLedger, InvocationTraceEvent};
use andromeda_observability::TraceId;
use andromeda_procedure_contract::PolicyVersion;
use andromeda_types::{CatalogVersion, ContractHash, InvocationId};

// Seed helpers so every test gets a distinct, deterministic TraceId.
const T1: TraceId = TraceId::new(0x_0001_0000_0000_0001);
const T2: TraceId = TraceId::new(0x_0001_0000_0000_0002);
const T3: TraceId = TraceId::new(0x_0001_0000_0000_0003);
const T4: TraceId = TraceId::new(0x_0001_0000_0000_0004);
const T5: TraceId = TraceId::new(0x_0001_0000_0000_0005);

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
            catalog_version: CatalogVersion::new(5),
            contract_hash: ContractHash::test_vector(0x5A),
            policy_version: PolicyVersion::new([0x5A; PolicyVersion::LEN]),
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
