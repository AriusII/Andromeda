use andromeda_error::AndromedaErrorKind;
use andromeda_execution_trace::CompletionMappingService;
use andromeda_observability::TraceId;
use andromeda_result_stream::CompletionStatus;
use andromeda_transaction::TransactionState;
use andromeda_types::InvocationId;
use andromeda_wal::Lsn;

#[test]
fn completion_mapping_rejects_commit_or_rollback_outcome_mismatches() {
    assert_eq!(
        CompletionMappingService::committed(
            InvocationId::new(8200),
            2,
            TransactionState::RolledBack,
            Lsn::new(10),
            TraceId::new(8200),
        )
        .unwrap_err()
        .kind(),
        AndromedaErrorKind::Transaction
    );

    assert_eq!(
        CompletionMappingService::committed(
            InvocationId::new(8201),
            2,
            TransactionState::Committed,
            Lsn::ZERO,
            TraceId::new(8201),
        )
        .unwrap_err()
        .kind(),
        AndromedaErrorKind::Storage
    );

    assert_eq!(
        CompletionMappingService::rolled_back(
            InvocationId::new(8202),
            TransactionState::Committed,
            Lsn::new(11),
            TraceId::new(8202),
        )
        .unwrap_err()
        .kind(),
        AndromedaErrorKind::Transaction
    );

    let rolled_back = CompletionMappingService::rolled_back(
        InvocationId::new(8203),
        TransactionState::RolledBack,
        Lsn::new(12),
        TraceId::new(8203),
    )
    .unwrap();
    assert_eq!(rolled_back.status(), CompletionStatus::RolledBack);
    assert_eq!(rolled_back.rows_affected(), Some(0));
    assert_eq!(
        rolled_back.transaction_state(),
        Some(TransactionState::RolledBack)
    );
}
