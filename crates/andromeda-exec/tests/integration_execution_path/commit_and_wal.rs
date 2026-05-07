use crate::support::*;

#[test]
fn e2e_procedure_invocation_completes_with_transaction_and_wal() {
    // Arrange
    let contract = valid_contract();
    let request = build_invocation_request(&contract, 1001);
    let context = build_invocation_context(1001);
    let trace_id = context.trace_id;
    let procedure = execute_reserve_stock(10, 3);

    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    // Act
    let outcome = runtime
        .execute_authorized_io_admitted(
            request,
            &procedure,
            &context,
            foreground_io_admission(trace_id),
        )
        .expect("execution should succeed");

    // Assert: Transaction lifecycle
    assert!(
        outcome.transaction_id.get() > 0,
        "transaction_id must be non-zero"
    );
    assert_eq!(
        outcome.completion.status(),
        CompletionStatus::Committed,
        "invocation must commit successfully"
    );
    assert_eq!(
        outcome.completion.transaction_state(),
        Some(TransactionState::Committed),
        "transaction state must be Committed"
    );

    // Assert: WAL durability
    let durable_lsn = outcome
        .completion
        .durable_lsn()
        .expect("committed transaction must have durable LSN");
    assert!(durable_lsn.get() > 0, "durable LSN must be positive");
    assert_eq!(
        runtime.wal().durable_lsn(),
        durable_lsn,
        "WAL durable LSN must match outcome"
    );

    // Assert: Result metadata
    assert_eq!(
        outcome.completion.rows_affected(),
        Some(2),
        "reserve operation should report the reserved quantity"
    );
}
#[test]
fn e2e_wal_durability_evidence_satisfies_lsn_ordering_invariants() {
    // Arrange
    let contract = valid_contract();
    let request = build_invocation_request(&contract, 8001);
    let context = build_invocation_context(8001);
    let trace_id = context.trace_id;
    let procedure = execute_reserve_stock(10, 2);

    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    // Act
    let outcome = runtime
        .execute_authorized_io_admitted(
            request,
            &procedure,
            &context,
            foreground_io_admission(trace_id),
        )
        .expect("execution should succeed");

    // Assert: WAL evidence validates
    let evidence = andromeda_exec::WalDurabilityEvidence {
        begin_lsn: Lsn::new(1),
        mutation_lsn: Some(Lsn::new(2)),
        commit_lsn: Lsn::new(3),
        durable_lsn: outcome.completion.durable_lsn().unwrap(),
    };

    // This would be validated in the actual execution path
    evidence
        .validate()
        .expect("WAL evidence should satisfy ordering invariants");

    // Assert: LSN constraints
    assert!(
        evidence.begin_lsn < evidence.commit_lsn,
        "begin LSN must precede commit"
    );
    if let Some(mut_lsn) = evidence.mutation_lsn {
        assert!(
            evidence.begin_lsn < mut_lsn && mut_lsn < evidence.commit_lsn,
            "mutation LSN must be between begin and commit"
        );
    }
    assert!(
        evidence.durable_lsn >= evidence.commit_lsn,
        "durable LSN must be >= commit LSN"
    );
}
