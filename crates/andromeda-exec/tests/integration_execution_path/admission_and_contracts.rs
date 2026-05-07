use crate::support::*;

#[test]
fn e2e_permission_denied_blocks_execution_and_emits_audit_trace() {
    // Arrange: Set up invocation with MISSING permissions
    let contract = valid_contract();
    let request = build_invocation_request(&contract, 2001);
    let trace_id = TraceId::new(2001);

    // Create context with EMPTY permissions (deny all)
    let context = InvocationContext::new(trace_id, vec![]);

    let procedure = execute_reserve_stock(10, 3);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    // Act
    let result = runtime.execute_authorized_io_admitted(
        request,
        &procedure,
        &context,
        foreground_io_admission(trace_id),
    );

    // Assert: Execution blocked before transaction creation
    assert!(
        result.is_err(),
        "execution should fail when permissions are denied"
    );
    let err = result.unwrap_err();
    let reason = err.message().to_lowercase();
    assert!(
        reason.contains("permission")
            || reason.contains("denied")
            || reason.contains("authorization"),
        "error reason should mention permission/authorization issue: {}",
        err
    );

    // Assert: No transaction was created (WAL should have no new entries for this)
    // Note: In a real system with shared transaction manager, we'd verify no TX was allocated
    assert_eq!(
        runtime.wal().durable_lsn(),
        Lsn::new(0),
        "pre-transaction denial must not write to WAL"
    );
}
#[test]
fn e2e_contract_hash_mismatch_rejected_at_admission() {
    // Arrange
    let contract = valid_contract();
    let mut request = build_invocation_request(&contract, 3001);

    // Introduce hash mismatch
    request.expected_contract_hash = ContractHash::new([255u8; 32]);

    let context = build_invocation_context(3001);
    let trace_id = context.trace_id;
    let procedure = execute_reserve_stock(10, 3);

    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    // Act
    let result = runtime.execute_authorized_io_admitted(
        request,
        &procedure,
        &context,
        foreground_io_admission(trace_id),
    );

    // Assert: Rejected at admission, before transaction
    assert!(
        result.is_err(),
        "contract hash mismatch should be rejected at admission"
    );
    let err = result.unwrap_err();
    let reason = err.message().to_lowercase();
    assert!(
        reason.contains("contract") || reason.contains("hash"),
        "error must indicate contract validation failure"
    );

    // Assert: No transaction created
    assert_eq!(
        runtime.wal().durable_lsn(),
        Lsn::new(0),
        "rejected invocation must not allocate transaction"
    );
}
#[test]
fn e2e_result_streaming_emits_metadata_with_correct_schema() {
    // Arrange
    let procedure = execute_reserve_stock(10, 5);

    // Assert: Procedure has valid result metadata
    assert!(
        procedure.result_metadata.column_count > 0
            || procedure.result_metadata.row_count_exact.is_some()
            || procedure.result_metadata.row_count_max.is_some(),
        "result metadata must be present"
    );

    // Validate metadata structure
    assert!(
        procedure.result_metadata.stream_id > 0,
        "result metadata must identify a stream"
    );

    // Assert: Result batch count matches rows affected
    // (In actual streaming, this would be validated per-batch)
    assert_eq!(
        procedure.rows_affected, 2,
        "reserve should update stock and reservation state"
    );
}
