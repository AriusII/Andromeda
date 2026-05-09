use andromeda_error::AndromedaErrorKind;
use andromeda_procedure_contract::{
    ResultRowCountSummary, RpcCompletion, RpcCompletionStatus, TransactionOutcome,
};
use andromeda_rpc_protocol::{
    BackpressureMetadata, ErrorEnvelope, ErrorFamily, RetryDisposition, TransactionEffect,
};
use andromeda_types::{RequestId, SessionId, TransactionId};

use super::proto_wire_fixtures::{RESERVATION_RESULT, RESERVATION_STREAM};

#[test]
fn completion_and_error_shapes_are_exposed_without_json_defaults() {
    let completion = RpcCompletion {
        request_id: Some(RequestId::new(101)),
        session_id: Some(SessionId::new(202)),
        trace_id: Some("trace-abc".to_string()),
        status: RpcCompletionStatus::Committed,
        transaction_outcome: TransactionOutcome::Committed,
        rows_affected: Some(42),
        result_row_counts: vec![ResultRowCountSummary {
            result_name: RESERVATION_STREAM.to_string(),
            rows_emitted: 1,
            row_count_exact: Some(1),
        }],
        tx_id: Some(TransactionId::new(77)),
        durable_lsn: Some(9001),
    };
    assert!(completion.validate().is_ok());
    assert_eq!(completion.status, RpcCompletionStatus::Committed);
    assert_eq!(completion.rows_affected, Some(42));
    assert_eq!(completion.tx_id, Some(TransactionId::new(77)));
    assert_eq!(completion.durable_lsn, Some(9001));

    let terminal_statuses = [
        RpcCompletionStatus::Committed,
        RpcCompletionStatus::RolledBack,
        RpcCompletionStatus::FailedBeforeTransaction,
        RpcCompletionStatus::Cancelled,
        RpcCompletionStatus::Poisoned,
        RpcCompletionStatus::PermissionDenied,
        RpcCompletionStatus::ContractRejected,
        RpcCompletionStatus::SystemUnavailable,
    ];
    assert_eq!(terminal_statuses.len(), 8);

    let error = ErrorEnvelope {
        request_id: Some(RequestId::new(101)),
        session_id: Some(SessionId::new(202)),
        trace_id: Some("trace-abc".to_string()),
        family: ErrorFamily::Contract,
        code: "CONTRACT_HASH_MISMATCH".to_string(),
        message: "ContractHash mismatch".to_string(),
        transaction_effect: TransactionEffect::NoTransaction,
        retry_disposition: RetryDisposition::Backpressure,
        retry_after_ms: Some(250),
        backpressure: Some(BackpressureMetadata {
            retry_after_ms: Some(250),
            capacity_percent: Some(95),
            shed_load: true,
        }),
    };
    assert!(error.validate().is_ok());
    assert_eq!(error.family, ErrorFamily::Contract);
    assert_eq!(error.code, "CONTRACT_HASH_MISMATCH");
    assert_eq!(error.transaction_effect, TransactionEffect::NoTransaction);
}

#[test]
fn committed_completion_requires_durable_lsn_evidence() {
    let missing_lsn = RpcCompletion {
        request_id: Some(RequestId::new(101)),
        session_id: Some(SessionId::new(202)),
        trace_id: Some("trace-abc".to_string()),
        status: RpcCompletionStatus::Committed,
        transaction_outcome: TransactionOutcome::Committed,
        rows_affected: Some(1),
        result_row_counts: Vec::new(),
        tx_id: Some(TransactionId::new(77)),
        durable_lsn: None,
    };

    assert_eq!(
        missing_lsn.validate().unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );

    let wrong_outcome = RpcCompletion {
        transaction_outcome: TransactionOutcome::RolledBack,
        durable_lsn: Some(1),
        ..missing_lsn
    };

    assert_eq!(
        wrong_outcome.validate().unwrap_err().kind(),
        AndromedaErrorKind::Transaction
    );
}

#[test]
fn reserve_stock_completion_requires_exact_row_count_and_transaction_outcome_evidence() {
    let committed = RpcCompletion {
        request_id: Some(RequestId::new(701)),
        session_id: Some(SessionId::new(702)),
        trace_id: Some("reserve-stock-trace".to_string()),
        status: RpcCompletionStatus::Committed,
        transaction_outcome: TransactionOutcome::Committed,
        rows_affected: Some(2),
        result_row_counts: vec![ResultRowCountSummary {
            result_name: RESERVATION_RESULT.to_string(),
            rows_emitted: 1,
            row_count_exact: Some(1),
        }],
        tx_id: Some(TransactionId::new(703)),
        durable_lsn: Some(704),
    };
    assert!(committed.validate().is_ok());

    let wrong_exact_row_count = RpcCompletion {
        result_row_counts: vec![ResultRowCountSummary {
            result_name: RESERVATION_RESULT.to_string(),
            rows_emitted: 1,
            row_count_exact: Some(0),
        }],
        ..committed.clone()
    };
    assert_eq!(
        wrong_exact_row_count.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let rolled_back = RpcCompletion {
        status: RpcCompletionStatus::RolledBack,
        transaction_outcome: TransactionOutcome::RolledBack,
        rows_affected: Some(0),
        result_row_counts: Vec::new(),
        ..committed.clone()
    };
    assert!(rolled_back.validate().is_ok());

    let rolled_back_with_rows = RpcCompletion {
        rows_affected: Some(1),
        ..rolled_back
    };
    assert_eq!(
        rolled_back_with_rows.validate().unwrap_err().kind(),
        AndromedaErrorKind::Transaction
    );

    let contract_rejected = RpcCompletion {
        status: RpcCompletionStatus::ContractRejected,
        transaction_outcome: TransactionOutcome::NotStarted,
        rows_affected: None,
        result_row_counts: Vec::new(),
        tx_id: None,
        durable_lsn: None,
        ..committed
    };
    assert!(contract_rejected.validate().is_ok());

    let contract_rejected_with_tx_evidence = RpcCompletion {
        tx_id: Some(TransactionId::new(703)),
        ..contract_rejected
    };
    assert_eq!(
        contract_rejected_with_tx_evidence
            .validate()
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );
}

#[test]
fn error_correlation_and_retry_metadata_are_validated() {
    let retry_after_without_delay = ErrorEnvelope {
        request_id: Some(RequestId::new(101)),
        session_id: Some(SessionId::new(202)),
        trace_id: Some("trace-abc".to_string()),
        family: ErrorFamily::Resource,
        code: "OVERLOADED".to_string(),
        message: "executor overloaded".to_string(),
        transaction_effect: TransactionEffect::NoTransaction,
        retry_disposition: RetryDisposition::RetryAfter,
        retry_after_ms: None,
        backpressure: None,
    };

    assert_eq!(
        retry_after_without_delay.validate().unwrap_err().kind(),
        AndromedaErrorKind::Resource
    );

    let empty_trace = ErrorEnvelope {
        trace_id: Some(" ".to_string()),
        retry_disposition: RetryDisposition::NotRetryable,
        ..retry_after_without_delay
    };

    assert_eq!(
        empty_trace.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );
}
