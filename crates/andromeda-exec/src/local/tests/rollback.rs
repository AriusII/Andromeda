use super::support::*;

#[test]
fn business_failure_routes_through_failed_before_durable_rollback() {
    let mut runtime = LocalVerticalRuntime::new(TestWal::default());
    let procedure = failure_procedure();

    let outcome = runtime
        .rollback_business_validation_failure_after_begin(
            failure_request(),
            &procedure,
            TraceId::new(9100),
            "insufficient inventory stock for reservation",
        )
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::RolledBack);
    assert_eq!(
        outcome.completion.transaction_state,
        Some(TransactionState::RolledBack)
    );
    assert_eq!(outcome.completion.rows_affected, Some(0));
    assert!(outcome.completion.durable_lsn.is_some());

    let wal = runtime.wal();
    assert_eq!(wal.records.len(), 2);
    assert_eq!(wal.records[0].1, WalRecordKind::TxBegin);
    assert_eq!(wal.records[1].1, WalRecordKind::TxRollback);
    assert!(
        !wal.records
            .iter()
            .any(|(_, kind, _, _)| *kind == WalRecordKind::TxCommit)
    );
    assert!(wal.durable_lsn >= wal.records[1].0);
    assert_eq!(
        wal.records[1].3.as_slice(),
        b"andromeda.exec.business-validation-failed.v1\0insufficient inventory stock for reservation"
    );

    let runtime_record = outcome.procedure_runtime_record();
    assert_eq!(runtime_record.binding, procedure.contract_binding);
    assert_eq!(runtime_record.status, ProcedureRuntimeStatus::RolledBack);
    assert_eq!(
        runtime_record.error_kind,
        Some(AndromedaErrorKind::Execution)
    );
    assert_eq!(runtime_record.counters.rows_written, 0);
    assert!(runtime_record.counters.rows_read > 0);
    assert!(runtime_record.counters.wal_bytes > 0);
    assert!(runtime_record.plan_key.is_some());
    assert!(runtime_record.plan_id.is_some());
}

#[test]
fn poison_failure_routes_through_poisoned_before_durable_rollback() {
    let mut runtime = LocalVerticalRuntime::new(TestWal::default());
    let procedure = failure_procedure();

    let outcome = runtime
        .rollback_poison_after_begin(
            failure_request(),
            &procedure,
            TraceId::new(9101),
            "engine invariant violated by post-begin runtime witness",
        )
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::RolledBack);
    assert_eq!(
        outcome.completion.transaction_state,
        Some(TransactionState::RolledBack)
    );
    assert_eq!(outcome.completion.rows_affected, Some(0));
    assert!(outcome.completion.durable_lsn.is_some());

    let wal = runtime.wal();
    assert_eq!(wal.records.len(), 2);
    assert_eq!(wal.records[0].1, WalRecordKind::TxBegin);
    assert_eq!(wal.records[1].1, WalRecordKind::TxRollback);
    assert!(
        !wal.records
            .iter()
            .any(|(_, kind, _, _)| *kind == WalRecordKind::TxCommit)
    );
    assert!(wal.durable_lsn >= wal.records[1].0);
    assert_eq!(
        wal.records[1].3.as_slice(),
        b"andromeda.exec.poisoned-rollback.v1\0engine invariant violated by post-begin runtime witness"
    );
}

#[test]
fn dispatcher_rollback_with_cause_records_intermediate_state() {
    for (cause, expected_intermediate) in [
        (RollbackCause::Direct, None),
        (
            RollbackCause::BusinessFailure,
            Some(TransactionState::Failed),
        ),
        (RollbackCause::Poison, Some(TransactionState::Poisoned)),
    ] {
        let mut wal = TestWal::default();
        let receipt = LocalDispatcher::new(&mut wal)
            .dispatch_rollback_with_cause(
                LocalRollbackPlan {
                    transaction_id: TransactionId::new(0xDEAD_BEEF),
                    rollback_payload: b"cause-routing".to_vec(),
                },
                cause,
            )
            .unwrap();

        assert_eq!(receipt.cause, cause);
        assert_eq!(receipt.intermediate_state, expected_intermediate);
        assert_eq!(receipt.transaction_state, TransactionState::RolledBack);
        assert!(receipt.durable_lsn >= receipt.wal_evidence.rollback_lsn);
        assert_eq!(wal.records[1].3.as_slice(), b"cause-routing");
    }
}

#[test]
fn poison_rollback_rejects_empty_reason() {
    let mut runtime = LocalVerticalRuntime::new(TestWal::default());
    let procedure = failure_procedure();

    let err = runtime
        .rollback_poison_after_begin(failure_request(), &procedure, TraceId::new(9102), "  ")
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Execution);
    assert!(runtime.wal().records.is_empty());
}
