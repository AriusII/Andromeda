use crate::common::*;

#[test]
fn local_vertical_runtime_can_require_io_admission_before_tx_begin() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let procedure = procedure(request(ContractHash::test_vector(7)).procedure);

    let outcome = runtime
        .execute_io_admitted(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(103),
            foreground_io_admission(TraceId::new(103)),
        )
        .unwrap();

    assert_eq!(outcome.completion.status(), CompletionStatus::Committed);
    assert_eq!(
        outcome.completion.transaction_state(),
        Some(TransactionState::Committed)
    );
    assert_eq!(runtime.wal().records.len(), 3);
}

#[test]
fn local_dispatcher_preserves_non_empty_mutation_payload_even_with_zero_rows_affected() {
    let mut wal = RecordingWal::default();
    let receipt = LocalDispatcher::new(&mut wal)
        .dispatch_commit(LocalDispatchPlan {
            transaction_id: TransactionId::new(406),
            mutation_payload: b"metadata-only-redo".to_vec(),
            rows_affected: 0,
        })
        .unwrap();

    assert_eq!(receipt.rows_affected, 0);
    assert_eq!(wal.records.len(), 3);
    assert_eq!(wal.records[0].1, WalRecordKind::TxBegin);
    assert_eq!(wal.records[1].1, WalRecordKind::RowUpdate);
    assert_eq!(wal.records[1].3.as_slice(), b"metadata-only-redo");
    assert_eq!(wal.records[2].1, WalRecordKind::TxCommit);
    assert_eq!(receipt.wal_evidence.mutation_lsn, Some(wal.records[1].0));
}

#[test]
fn local_vertical_runtime_rejects_io_admission_trace_mismatch_before_tx_begin() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let procedure = procedure(request(ContractHash::test_vector(7)).procedure);

    let err = runtime
        .execute_io_admitted(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(104),
            foreground_io_admission(TraceId::new(105)),
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_error::AndromedaErrorKind::Contract);
    assert!(err.message().contains("trace id"));
    assert!(runtime.wal().records.is_empty());
}

#[test]
fn local_vertical_runtime_blocks_core_io_guarded_execution_when_io_admission_rejects() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let procedure = procedure(request(ContractHash::test_vector(7)).procedure);

    let err = runtime
        .execute_io_admitted(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(105),
            rejected_io_admission(TraceId::new(106)),
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_error::AndromedaErrorKind::Resource);
    assert!(err.message().contains("execution IO admission rejected"));
    assert!(err.message().contains("memory budget must not be zero"));
    assert!(runtime.wal().records.is_empty());
}

#[test]
fn contract_or_procedure_mismatch_is_rejected_before_tx_begin() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let mismatched_procedure = procedure(ProcedureContractRef {
        procedure_id: ProcedureId::new(99),
        ..request(ContractHash::test_vector(7)).procedure
    });

    let err = runtime
        .execute(
            request(ContractHash::test_vector(7)),
            &mismatched_procedure,
            TraceId::new(101),
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_error::AndromedaErrorKind::Contract);
    assert!(runtime.wal().records.is_empty());
}

#[test]
fn executable_binding_stats_drift_is_rejected_before_tx_begin() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let mut drifted_procedure = procedure(request(ContractHash::test_vector(7)).procedure);
    drifted_procedure.contract_binding.stats_version = StatsVersion::new(2);

    let err = runtime
        .execute(
            request(ContractHash::test_vector(7)),
            &drifted_procedure,
            TraceId::new(109),
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_error::AndromedaErrorKind::Contract);
    assert!(err.message().contains("StatsVersion"));
    assert!(runtime.wal().records.is_empty());
}
