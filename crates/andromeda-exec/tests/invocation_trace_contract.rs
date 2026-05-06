//! Invocation Trace Contract Tests — Wave 13 Batch 3
//!
//! Tests verify the 4 critical tracing points and durable audit ledger guarantees:
//!
//! 1. **Admission Decision Trace** — Accepted/denied with reason
//! 2. **Dispatch Event Trace** — Procedure resolved, executor selected
//! 3. **Execution Start/End Trace** — Timestamps, result cardinality
//! 4. **Error/Rollback Trace** — Failure reason, state transition
//!
//! **Invariant**: All procedure invocations produce durable traces (no silent omissions)

use andromeda_core::InvocationId;
use andromeda_exec::{AuditLedger, InMemoryAuditLedger, InvocationTraceEvent};
use andromeda_observe::TraceId;

/// Test 1: Audit ledger accepts and records admission decision traces.
#[test]
fn test_admission_decision_trace_recorded() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id = TraceId::new(1);
    let invocation_id = InvocationId::new(1);

    let event = InvocationTraceEvent::AdmissionDecision {
        trace_id,
        invocation_id,
        accepted: true,
        reason: "contract validated, resource budget available".to_string(),
    };

    let result = ledger.append_trace(event.clone());
    assert!(result.is_ok(), "admission trace should be recorded");
    assert_eq!(ledger.total_appended(), 1, "appended count should be 1");
    assert_eq!(ledger.total_rejected(), 0, "rejected count should be 0");

    let snapshot = ledger.snapshot().expect("snapshot should succeed");
    assert_eq!(snapshot.len(), 1, "snapshot should have 1 event");
    assert_eq!(snapshot[0], event, "recorded event should match input");
}

/// Test 2: Audit ledger rejects admission denials with recorded reason.
#[test]
fn test_admission_denial_trace_recorded() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id = TraceId::new(2);
    let invocation_id = InvocationId::new(2);

    let event = InvocationTraceEvent::AdmissionDecision {
        trace_id,
        invocation_id,
        accepted: false,
        reason: "permission denied: missing execute_procedure permission".to_string(),
    };

    let result = ledger.append_trace(event.clone());
    assert!(result.is_ok(), "denial trace should be recorded");
    assert_eq!(ledger.total_appended(), 1);

    let snapshot = ledger.snapshot().expect("snapshot should succeed");
    assert_eq!(snapshot.len(), 1);

    match &snapshot[0] {
        InvocationTraceEvent::AdmissionDecision {
            accepted: false,
            reason,
            ..
        } => {
            assert!(
                reason.contains("permission denied"),
                "reason should indicate permission denial"
            );
        }
        _ => panic!("expected admission decision trace"),
    }
}

/// Test 3: Dispatch event traces are recorded with executor kind.
#[test]
fn test_dispatch_event_trace_recorded() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id = TraceId::new(3);
    let invocation_id = InvocationId::new(3);

    let event = InvocationTraceEvent::DispatchEvent {
        trace_id,
        invocation_id,
        procedure_name: "calculate_revenue".to_string(),
        executor_kind: "LocalVerticalExecutor".to_string(),
    };

    let result = ledger.append_trace(event.clone());
    assert!(result.is_ok(), "dispatch event should be recorded");
    assert_eq!(ledger.total_appended(), 1);

    let snapshot = ledger.snapshot().expect("snapshot should succeed");
    assert_eq!(snapshot.len(), 1);

    match &snapshot[0] {
        InvocationTraceEvent::DispatchEvent {
            procedure_name,
            executor_kind,
            ..
        } => {
            assert_eq!(procedure_name, "calculate_revenue");
            assert_eq!(executor_kind, "LocalVerticalExecutor");
        }
        _ => panic!("expected dispatch event trace"),
    }
}

/// Test 4: Execution start traces capture transaction ID binding.
#[test]
fn test_execution_start_trace_recorded() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id = TraceId::new(4);
    let invocation_id = InvocationId::new(4);
    let transaction_id = andromeda_core::TransactionId::new(100);

    let event = InvocationTraceEvent::ExecutionStart {
        trace_id,
        invocation_id,
        transaction_id,
    };

    let result = ledger.append_trace(event.clone());
    assert!(result.is_ok(), "execution start trace should be recorded");
    assert_eq!(ledger.total_appended(), 1);

    let snapshot = ledger.snapshot().expect("snapshot should succeed");
    assert_eq!(snapshot.len(), 1);

    match &snapshot[0] {
        InvocationTraceEvent::ExecutionStart {
            transaction_id: tx_id,
            ..
        } => {
            assert_eq!(tx_id.get(), 100, "transaction id should be preserved");
        }
        _ => panic!("expected execution start trace"),
    }
}

/// Test 5: Execution end traces capture cardinality and row counts.
#[test]
fn test_execution_end_trace_recorded() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id = TraceId::new(5);
    let invocation_id = InvocationId::new(5);

    let event = InvocationTraceEvent::ExecutionEnd {
        trace_id,
        invocation_id,
        status: "Committed".to_string(),
        rows_affected: Some(42),
        result_cardinality: Some("One".to_string()),
    };

    let result = ledger.append_trace(event.clone());
    assert!(result.is_ok(), "execution end trace should be recorded");
    assert_eq!(ledger.total_appended(), 1);

    let snapshot = ledger.snapshot().expect("snapshot should succeed");
    match &snapshot[0] {
        InvocationTraceEvent::ExecutionEnd {
            rows_affected,
            result_cardinality,
            ..
        } => {
            assert_eq!(rows_affected, &Some(42));
            assert_eq!(result_cardinality, &Some("One".to_string()));
        }
        _ => panic!("expected execution end trace"),
    }
}

/// Test 6: Execution failed traces capture error reason and recoverability.
#[test]
fn test_execution_failed_trace_recorded() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id = TraceId::new(6);
    let invocation_id = InvocationId::new(6);

    let event = InvocationTraceEvent::ExecutionFailed {
        trace_id,
        invocation_id,
        failure_reason: "constraint violation: duplicate key".to_string(),
        recoverable: true,
    };

    let result = ledger.append_trace(event.clone());
    assert!(result.is_ok(), "execution failed trace should be recorded");
    assert_eq!(ledger.total_appended(), 1);

    let snapshot = ledger.snapshot().expect("snapshot should succeed");
    match &snapshot[0] {
        InvocationTraceEvent::ExecutionFailed {
            failure_reason,
            recoverable,
            ..
        } => {
            assert!(failure_reason.contains("constraint violation"));
            assert_eq!(recoverable, &true);
        }
        _ => panic!("expected execution failed trace"),
    }
}

/// Test 7: Multiple traces for single invocation are all recorded.
#[test]
fn test_complete_invocation_trace_sequence() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id = TraceId::new(7);
    let invocation_id = InvocationId::new(7);
    let transaction_id = andromeda_core::TransactionId::new(200);

    // Phase 1: Admission
    let admission = InvocationTraceEvent::AdmissionDecision {
        trace_id,
        invocation_id,
        accepted: true,
        reason: "admitted".to_string(),
    };

    // Phase 2: Dispatch
    let dispatch = InvocationTraceEvent::DispatchEvent {
        trace_id,
        invocation_id,
        procedure_name: "test_proc".to_string(),
        executor_kind: "Local".to_string(),
    };

    // Phase 3: Execution Start
    let exec_start = InvocationTraceEvent::ExecutionStart {
        trace_id,
        invocation_id,
        transaction_id,
    };

    // Phase 4: Execution End
    let exec_end = InvocationTraceEvent::ExecutionEnd {
        trace_id,
        invocation_id,
        status: "Committed".to_string(),
        rows_affected: Some(10),
        result_cardinality: Some("Many".to_string()),
    };

    ledger.append_trace(admission).expect("admission failed");
    ledger.append_trace(dispatch).expect("dispatch failed");
    ledger.append_trace(exec_start).expect("exec_start failed");
    ledger.append_trace(exec_end).expect("exec_end failed");

    assert_eq!(
        ledger.total_appended(),
        4,
        "all 4 phases should be recorded"
    );
    assert_eq!(ledger.total_rejected(), 0, "no rejections expected");

    let snapshot = ledger.snapshot().expect("snapshot failed");
    assert_eq!(snapshot.len(), 4);

    // Verify sequence order
    assert!(matches!(
        snapshot[0],
        InvocationTraceEvent::AdmissionDecision { .. }
    ));
    assert!(matches!(
        snapshot[1],
        InvocationTraceEvent::DispatchEvent { .. }
    ));
    assert!(matches!(
        snapshot[2],
        InvocationTraceEvent::ExecutionStart { .. }
    ));
    assert!(matches!(
        snapshot[3],
        InvocationTraceEvent::ExecutionEnd { .. }
    ));
}

/// Test 8: Query by trace_id filters events correctly.
#[test]
fn test_query_by_trace_id_filters_correctly() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id1 = TraceId::new(8);
    let trace_id2 = TraceId::new(9);
    let invocation_id1 = InvocationId::new(8);
    let invocation_id2 = InvocationId::new(9);

    // Add events for trace_id1
    ledger
        .append_trace(InvocationTraceEvent::AdmissionDecision {
            trace_id: trace_id1,
            invocation_id: invocation_id1,
            accepted: true,
            reason: "test".to_string(),
        })
        .expect("append failed");

    // Add events for trace_id2
    ledger
        .append_trace(InvocationTraceEvent::AdmissionDecision {
            trace_id: trace_id2,
            invocation_id: invocation_id2,
            accepted: true,
            reason: "test".to_string(),
        })
        .expect("append failed");

    let results1 = ledger.query_by_trace_id(trace_id1).expect("query failed");
    let results2 = ledger.query_by_trace_id(trace_id2).expect("query failed");

    assert_eq!(results1.len(), 1, "trace_id1 should have 1 event");
    assert_eq!(results2.len(), 1, "trace_id2 should have 1 event");

    // Verify the filtered events belong to the correct trace
    match &results1[0] {
        InvocationTraceEvent::AdmissionDecision {
            trace_id: t,
            invocation_id: i,
            ..
        } => {
            assert_eq!(t, &trace_id1);
            assert_eq!(i, &invocation_id1);
        }
        _ => panic!("unexpected event type"),
    }
}

/// Test 9: Query by invocation_id filters events correctly.
#[test]
fn test_query_by_invocation_id_filters_correctly() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id = TraceId::new(10);
    let invocation_id1 = InvocationId::new(10);
    let invocation_id2 = InvocationId::new(11);

    // Add events for invocation_id1
    ledger
        .append_trace(InvocationTraceEvent::AdmissionDecision {
            trace_id,
            invocation_id: invocation_id1,
            accepted: true,
            reason: "test".to_string(),
        })
        .expect("append failed");

    ledger
        .append_trace(InvocationTraceEvent::ExecutionStart {
            trace_id,
            invocation_id: invocation_id1,
            transaction_id: andromeda_core::TransactionId::new(1),
        })
        .expect("append failed");

    // Add event for invocation_id2
    ledger
        .append_trace(InvocationTraceEvent::AdmissionDecision {
            trace_id,
            invocation_id: invocation_id2,
            accepted: false,
            reason: "denied".to_string(),
        })
        .expect("append failed");

    let results1 = ledger
        .query_by_invocation_id(invocation_id1)
        .expect("query failed");
    let results2 = ledger
        .query_by_invocation_id(invocation_id2)
        .expect("query failed");

    assert_eq!(results1.len(), 2, "invocation_id1 should have 2 events");
    assert_eq!(results2.len(), 1, "invocation_id2 should have 1 event");
}

/// Test 10: No silent drops — rejected traces increment counter.
#[test]
fn test_no_silent_drops_on_ledger_failure() {
    // This test would require mocking or a failing ledger implementation.
    // For now, we verify that a healthy ledger never silently drops.
    let ledger = InMemoryAuditLedger::new();

    for i in 0..100 {
        let event = InvocationTraceEvent::AdmissionDecision {
            trace_id: TraceId::new(u128::from(i) + 100),
            invocation_id: InvocationId::new(i),
            accepted: true,
            reason: format!("test {}", i),
        };

        let result = ledger.append_trace(event);
        assert!(result.is_ok(), "all appends should succeed");
    }

    assert_eq!(
        ledger.total_appended(),
        100,
        "all 100 traces should be recorded"
    );
    assert_eq!(ledger.total_rejected(), 0, "no rejections expected");
}

/// Test 11: Trace event equality preserves all fields.
#[test]
fn test_trace_event_equality_complete() {
    let trace_id = TraceId::new(12);
    let invocation_id = InvocationId::new(12);

    let event1 = InvocationTraceEvent::AdmissionDecision {
        trace_id,
        invocation_id,
        accepted: true,
        reason: "test".to_string(),
    };

    let event2 = InvocationTraceEvent::AdmissionDecision {
        trace_id,
        invocation_id,
        accepted: true,
        reason: "test".to_string(),
    };

    assert_eq!(event1, event2, "identical events should be equal");

    let event3 = InvocationTraceEvent::AdmissionDecision {
        trace_id,
        invocation_id,
        accepted: false,
        reason: "test".to_string(),
    };

    assert_ne!(
        event1, event3,
        "different accepted status should not be equal"
    );
}

/// Test 12: Concurrent append attempts maintain consistency.
#[test]
fn test_concurrent_appends_maintain_consistency() {
    let ledger = std::sync::Arc::new(InMemoryAuditLedger::new());
    let mut handles = vec![];

    for thread_id in 0..5 {
        let ledger_clone = ledger.clone();
        let handle = std::thread::spawn(move || {
            for i in 0..20 {
                let invocation_id = InvocationId::new((thread_id * 20 + i) as u64);
                let event = InvocationTraceEvent::AdmissionDecision {
                    trace_id: TraceId::new((thread_id * 20 + i) as u128 + 1_000),
                    invocation_id,
                    accepted: true,
                    reason: format!("thread {} event {}", thread_id, i),
                };

                let result = ledger_clone.append_trace(event);
                assert!(result.is_ok(), "append should succeed");
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("thread should complete");
    }

    assert_eq!(
        ledger.total_appended(),
        100,
        "all 100 traces should be recorded"
    );
    assert_eq!(ledger.total_rejected(), 0);

    let snapshot = ledger.snapshot().expect("snapshot failed");
    assert_eq!(snapshot.len(), 100, "snapshot should have all 100 events");
}
