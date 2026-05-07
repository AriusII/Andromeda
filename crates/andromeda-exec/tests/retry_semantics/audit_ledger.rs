use crate::support::execution_failed;
use andromeda_core::InvocationId;
use andromeda_exec::traces::{AuditLedger, InMemoryAuditLedger};
use andromeda_observe::TraceId;

#[test]
fn test_audit_ledger_append_and_retrieve() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id = TraceId::new(1);
    let invocation_id = InvocationId::new(1);
    let event = execution_failed(trace_id, invocation_id, "test failure");

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
    let trace_id = TraceId::new(2);
    let invocation_id = InvocationId::new(2);

    for i in 0..5 {
        assert!(
            ledger
                .append_trace(execution_failed(
                    trace_id,
                    invocation_id,
                    format!("failure {}", i),
                ))
                .is_ok()
        );
    }

    assert_eq!(ledger.total_appended(), 5);
    let snapshot = ledger.snapshot().unwrap();
    assert_eq!(snapshot.len(), 5);
}

#[test]
fn test_audit_ledger_query_by_trace_id() {
    let ledger = InMemoryAuditLedger::new();
    let trace_id_1 = TraceId::new(3);
    let trace_id_2 = TraceId::new(4);
    let invocation_id = InvocationId::new(3);

    let event1 = execution_failed(trace_id_1, invocation_id, "error 1");
    let event2 = execution_failed(trace_id_2, invocation_id, "error 2");

    assert!(ledger.append_trace(event1.clone()).is_ok());
    assert!(ledger.append_trace(event2).is_ok());

    let results = ledger.query_by_trace_id(trace_id_1).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], event1);
}
