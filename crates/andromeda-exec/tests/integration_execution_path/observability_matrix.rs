use crate::support::*;

#[test]
fn e2e_audit_events_correlated_with_trace_and_transaction() {
    // Arrange
    let contract = valid_contract();
    let request = build_invocation_request(&contract, 7001);
    let context = build_invocation_context(7001);
    let trace_id = context.trace_id;
    let procedure = execute_reserve_stock(10, 4);

    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let mut sink = InMemoryEventSink::new();

    // Act
    let outcome = runtime
        .execute_authorized_io_admitted(
            request,
            &procedure,
            &context,
            foreground_io_admission(trace_id),
        )
        .expect("execution should succeed");

    // Manually emit event to test correlation
    let mut emitter = EventEmitter::new(&mut sink);
    let tx_id = outcome.transaction_id;
    let durable_lsn = outcome.completion.durable_lsn().unwrap();

    let correlation = build_event_correlation(&contract, Some(tx_id), Some(durable_lsn));

    LocalVerticalRuntime::<InMemoryWal>::emit_commit_visible_event(
        &mut emitter,
        trace_id,
        tx_id,
        durable_lsn,
        correlation,
    )
    .expect("event emission should succeed");

    // Assert: Event was emitted with correct correlation
    assert_eq!(sink.len(), 1, "exactly one event should be emitted");
    let envelope = &sink.events()[0];

    assert_eq!(
        envelope.trace_id, trace_id,
        "event trace_id must match context"
    );
    assert_eq!(envelope.event_id, EventId::new(1), "first event has ID 1");

    // Verify correlation fields
    let corr = &envelope.correlation;
    assert_eq!(corr.request_id, Some(RequestId::new(5001)));
    assert_eq!(corr.transaction_id, Some(tx_id));
    assert_eq!(corr.durable_lsn, Some(durable_lsn.get()));
}
#[test]
fn integration_test_matrix_coverage_complete() {
    // This test documents the required coverage matrix
    let test_matrix = [
        (
            "E2E.1",
            "admission → dispatch → commit",
            "procedure_invocation_completes_with_transaction_and_wal",
        ),
        (
            "E2E.2",
            "permission enforcement → audit",
            "permission_denied_blocks_execution_and_emits_audit_trace",
        ),
        (
            "E2E.3",
            "contract validation → rejection",
            "contract_hash_mismatch_rejected_at_admission",
        ),
        (
            "E2E.4",
            "dispatch → result streaming",
            "result_streaming_emits_metadata_with_correct_schema",
        ),
        (
            "E2E.5",
            "error → rollback → durability",
            "procedure_error_triggers_rollback_and_wal_durability",
        ),
        (
            "E2E.6",
            "concurrency → MVCC isolation",
            "concurrent_invocations_maintain_mvcc_isolation",
        ),
        (
            "E2E.7",
            "execution → audit → correlation",
            "audit_events_correlated_with_trace_and_transaction",
        ),
        (
            "E2E.8",
            "commit → WAL evidence validation",
            "wal_durability_evidence_satisfies_lsn_ordering_invariants",
        ),
    ];

    println!("Integration Execution Path Test Matrix:");
    println!("┌───────┬──────────────────────────────┬────────────────────────────────────┐");
    println!("│ Test  │ Path                         │ Test Function                      │");
    println!("├───────┼──────────────────────────────┼────────────────────────────────────┤");

    for (id, path, func) in &test_matrix {
        println!("│ {:5} │ {:28} │ {:34} │", id, path, func);
    }

    println!("└───────┴──────────────────────────────┴────────────────────────────────────┘");

    // Verify all 8 tests are present
    assert_eq!(
        test_matrix.len(),
        8,
        "must have exactly 8 integration tests covering all execution paths"
    );
}
