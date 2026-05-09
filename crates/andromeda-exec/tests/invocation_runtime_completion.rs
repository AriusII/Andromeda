use andromeda_error::AndromedaResult;
use andromeda_execution_trace::{
    CompletionEmission, CompletionJournalRecord, CompletionMappingService,
    CompletionRecoveryExpectation, CompletionRecoveryStatus, InvocationCompletionEmitter,
    reconcile_completion_recovery_from_wal,
};
use andromeda_observe::TraceId;
use andromeda_result_stream::CompletionStatus;
use andromeda_transaction::TransactionState;
use andromeda_types::{InvocationId, TransactionId};
use andromeda_wal::{Lsn, WalRecord, WalRecordKind};

fn wal_record(
    kind: WalRecordKind,
    lsn: u64,
    previous_lsn: Option<u64>,
    transaction_id: Option<TransactionId>,
) -> AndromedaResult<WalRecord> {
    WalRecord::from_parts(
        kind,
        Lsn::new(lsn),
        previous_lsn.map(Lsn::new),
        transaction_id,
        Vec::new(),
    )
}

#[test]
fn successful_invocation_emits_exactly_one_completion() -> AndromedaResult<()> {
    let invocation_id = InvocationId::new(11);
    let transaction_id = TransactionId::new(101);
    let commit_lsn = Lsn::new(7);
    let durable_lsn = Lsn::new(9);
    let completion = CompletionMappingService::committed(
        invocation_id,
        3,
        TransactionState::Committed,
        durable_lsn,
        TraceId::new(1),
    )?;
    let mut emitter = InvocationCompletionEmitter::new();

    let record = emitter.emit(CompletionEmission::committed(
        completion,
        transaction_id,
        commit_lsn,
        Some(2),
    ))?;

    assert_eq!(record.invocation_id, invocation_id);
    assert_eq!(record.transaction_id, Some(transaction_id));
    assert_eq!(record.status, CompletionStatus::Committed);
    assert_eq!(record.transaction_state, Some(TransactionState::Committed));
    assert_eq!(record.rows_affected, Some(3));
    assert_eq!(record.result_row_count_exact, Some(2));
    assert_eq!(record.terminal_lsn, Some(commit_lsn));
    assert_eq!(record.durable_lsn, Some(durable_lsn));
    assert_eq!(emitter.journal().len(), 1);
    assert!(emitter.get(invocation_id).is_some());

    Ok(())
}

#[test]
fn duplicate_completion_is_rejected() -> AndromedaResult<()> {
    let invocation_id = InvocationId::new(12);
    let transaction_id = TransactionId::new(102);
    let completion = CompletionMappingService::committed(
        invocation_id,
        1,
        TransactionState::Committed,
        Lsn::new(12),
        TraceId::new(2),
    )?;
    let mut emitter = InvocationCompletionEmitter::new();

    emitter.emit(CompletionEmission::committed(
        completion,
        transaction_id,
        Lsn::new(10),
        Some(1),
    ))?;

    let duplicate = emitter
        .emit(CompletionEmission::committed(
            completion,
            transaction_id,
            Lsn::new(11),
            Some(1),
        ))
        .unwrap_err();

    assert!(duplicate.to_string().contains("already emitted"));
    assert_eq!(emitter.journal().len(), 1);

    Ok(())
}

#[test]
fn invalid_completion_emission_is_rejected_before_audit_or_journal() -> AndromedaResult<()> {
    let completion = CompletionMappingService::rejected(
        InvocationId::new(121),
        CompletionStatus::PermissionDenied,
        TraceId::new(121),
    )?;
    let emission =
        CompletionEmission::committed(completion, TransactionId::new(1201), Lsn::new(12), None);
    let mut emitter = InvocationCompletionEmitter::new();

    let error = emitter.emit(emission).unwrap_err();

    assert!(
        error
            .message()
            .contains("committed completion emission requires committed completion status")
    );
    assert!(emitter.journal().is_empty());
    Ok(())
}

#[test]
fn duplicate_transaction_completion_is_rejected_across_invocations() -> AndromedaResult<()> {
    let transaction_id = TransactionId::new(103);
    let mut emitter = InvocationCompletionEmitter::new();

    let first = CompletionMappingService::committed(
        InvocationId::new(13),
        1,
        TransactionState::Committed,
        Lsn::new(13),
        TraceId::new(3),
    )?;
    emitter.emit(CompletionEmission::committed(
        first,
        transaction_id,
        Lsn::new(13),
        Some(1),
    ))?;

    let second = CompletionMappingService::committed(
        InvocationId::new(14),
        1,
        TransactionState::Committed,
        Lsn::new(14),
        TraceId::new(4),
    )?;
    let duplicate = emitter
        .emit(CompletionEmission::committed(
            second,
            transaction_id,
            Lsn::new(14),
            Some(1),
        ))
        .unwrap_err();

    assert!(duplicate.to_string().contains("transaction completion"));
    assert_eq!(emitter.journal().len(), 1);
    assert!(emitter.get(InvocationId::new(14)).is_none());

    Ok(())
}

#[test]
fn permission_denied_and_contract_rejected_are_stripped_from_tx_and_lsn() -> AndromedaResult<()> {
    for (offset, status) in [
        (0, CompletionStatus::PermissionDenied),
        (1, CompletionStatus::ContractRejected),
    ] {
        let invocation_id = InvocationId::new(20 + offset);
        let completion = CompletionMappingService::rejected(
            invocation_id,
            status,
            TraceId::new(20 + u128::from(offset)),
        )?;
        let mut emitter = InvocationCompletionEmitter::new();

        let record = emitter.emit(CompletionEmission::pre_transaction(completion))?;

        assert_eq!(record.invocation_id, invocation_id);
        assert_eq!(record.status, status);
        assert_eq!(record.transaction_id, None);
        assert_eq!(record.transaction_state, None);
        assert_eq!(record.rows_affected, None);
        assert_eq!(record.result_row_count_exact, None);
        assert_eq!(record.terminal_lsn, None);
        assert_eq!(record.durable_lsn, None);
    }

    Ok(())
}

#[test]
fn non_transactional_journal_records_reject_result_metadata() {
    let record = CompletionJournalRecord {
        invocation_id: InvocationId::new(29),
        transaction_id: None,
        status: CompletionStatus::PermissionDenied,
        transaction_state: None,
        rows_affected: None,
        result_row_count_exact: Some(1),
        terminal_lsn: None,
        durable_lsn: None,
        trace_id: TraceId::new(29),
    };

    let error = record.validate().unwrap_err();

    assert!(error.message().contains(
        "non-transactional journal record must not carry transaction or result evidence"
    ));
}

#[test]
fn poison_after_tx_begin_emits_durable_rolled_back_completion() -> AndromedaResult<()> {
    let invocation_id = InvocationId::new(31);
    let transaction_id = TransactionId::new(301);
    let rollback_lsn = Lsn::new(32);
    let durable_lsn = Lsn::new(33);
    let completion = CompletionMappingService::poisoned_after_rollback(
        invocation_id,
        TransactionState::RolledBack,
        durable_lsn,
        TraceId::new(31),
    )?;
    let mut emitter = InvocationCompletionEmitter::new();

    assert_eq!(completion.status(), CompletionStatus::RolledBack);
    assert_eq!(
        completion.transaction_state(),
        Some(TransactionState::RolledBack)
    );
    assert_eq!(completion.rows_affected(), Some(0));
    assert_eq!(completion.durable_lsn(), Some(durable_lsn));

    let record = emitter.emit(CompletionEmission::rolled_back(
        completion,
        transaction_id,
        rollback_lsn,
    ))?;

    assert_eq!(record.invocation_id, invocation_id);
    assert_eq!(record.transaction_id, Some(transaction_id));
    assert_eq!(record.status, CompletionStatus::RolledBack);
    assert_eq!(record.transaction_state, Some(TransactionState::RolledBack));
    assert_eq!(record.rows_affected, Some(0));
    assert_eq!(record.result_row_count_exact, Some(0));
    assert_eq!(record.terminal_lsn, Some(rollback_lsn));
    assert_eq!(record.durable_lsn, Some(durable_lsn));

    Ok(())
}

#[test]
fn completion_recovery_treats_duplicate_commit_wal_as_idempotent() -> AndromedaResult<()> {
    let invocation_id = InvocationId::new(41);
    let transaction_id = TransactionId::new(401);
    let first_commit_lsn = Lsn::new(42);
    let durable_lsn = Lsn::new(43);
    let completion = CompletionMappingService::committed(
        invocation_id,
        3,
        TransactionState::Committed,
        durable_lsn,
        TraceId::new(41),
    )?;
    let journal_record =
        CompletionJournalRecord::committed(completion, transaction_id, first_commit_lsn, Some(2))?;
    let records = vec![
        wal_record(WalRecordKind::TxBegin, 41, None, Some(transaction_id))?,
        wal_record(WalRecordKind::TxCommit, 42, Some(41), Some(transaction_id))?,
        wal_record(WalRecordKind::TxCommit, 43, Some(42), Some(transaction_id))?,
    ];
    let expectation = CompletionRecoveryExpectation::for_invocation_with_transaction(
        invocation_id,
        transaction_id,
    )
    .with_expected_metadata(Some(3), Some(2))
    .with_journal_record(journal_record);

    let report = reconcile_completion_recovery_from_wal(&records, &[expectation])?;

    assert!(!report.has_ambiguity());
    assert_eq!(report.durable_lsn, durable_lsn);
    assert_eq!(report.records.len(), 1);
    assert_eq!(
        report.records[0].status,
        CompletionRecoveryStatus::Completed
    );
    assert_eq!(report.records[0].terminal_lsn, Some(first_commit_lsn));
    assert_eq!(report.records[0].rows_affected, Some(3));
    assert_eq!(report.records[0].result_row_count_exact, Some(2));
    Ok(())
}

#[test]
fn completion_recovery_rejects_journal_terminal_not_present_in_wal() -> AndromedaResult<()> {
    let invocation_id = InvocationId::new(42);
    let transaction_id = TransactionId::new(402);
    let completion = CompletionMappingService::committed(
        invocation_id,
        1,
        TransactionState::Committed,
        Lsn::new(52),
        TraceId::new(42),
    )?;
    let journal_record =
        CompletionJournalRecord::committed(completion, transaction_id, Lsn::new(51), Some(1))?;
    let records = vec![
        wal_record(WalRecordKind::TxBegin, 50, None, Some(transaction_id))?,
        wal_record(WalRecordKind::TxCommit, 52, Some(50), Some(transaction_id))?,
    ];
    let expectation = CompletionRecoveryExpectation::for_invocation_with_transaction(
        invocation_id,
        transaction_id,
    )
    .with_journal_record(journal_record);

    let report = reconcile_completion_recovery_from_wal(&records, &[expectation])?;

    assert!(report.has_ambiguity());
    assert_eq!(
        report.records[0].status,
        CompletionRecoveryStatus::Ambiguous
    );
    Ok(())
}

#[test]
fn rejected_completion_cannot_project_transactional_terminal_status() {
    let err = CompletionMappingService::rejected(
        InvocationId::new(32),
        CompletionStatus::RolledBack,
        TraceId::new(32),
    )
    .unwrap_err();

    assert!(
        err.to_string()
            .contains("cannot use transactional terminal status")
    );
}

#[test]
fn rolledback_without_durable_lsn_is_rejected() {
    let err = CompletionMappingService::rolled_back(
        InvocationId::new(30),
        TransactionState::RolledBack,
        Lsn::ZERO,
        TraceId::new(30),
    )
    .unwrap_err();

    assert!(err.to_string().contains("durable WAL LSN evidence"));
}
