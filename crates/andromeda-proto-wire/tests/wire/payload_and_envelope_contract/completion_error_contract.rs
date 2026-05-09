use andromeda_error::AndromedaErrorKind;
use andromeda_procedure_contract::{
    RPC_COMPLETION_STATUS_TERMINAL_CODES, ResultRowCountSummary, RpcCompletion,
    RpcCompletionStatus, TRANSACTION_OUTCOME_TERMINAL_CODES, TransactionOutcome,
};
use andromeda_proto_wire::{
    GeneratedRpcCompletionStatus, GeneratedTransactionOutcome,
    generated_validation::{GeneratedBackpressureMetadataView, GeneratedErrorEnvelopeView},
    validate_generated_error_envelope,
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
fn generated_completion_codes_lockstep_with_procedure_contract() {
    let generated_statuses = [
        GeneratedRpcCompletionStatus::Committed,
        GeneratedRpcCompletionStatus::RolledBack,
        GeneratedRpcCompletionStatus::FailedBeforeTransaction,
        GeneratedRpcCompletionStatus::Cancelled,
        GeneratedRpcCompletionStatus::Poisoned,
        GeneratedRpcCompletionStatus::PermissionDenied,
        GeneratedRpcCompletionStatus::ContractRejected,
        GeneratedRpcCompletionStatus::SystemUnavailable,
    ];
    assert_eq!(
        generated_statuses.len(),
        RPC_COMPLETION_STATUS_TERMINAL_CODES.len()
    );
    for ((contract_status, contract_code), generated_status) in RPC_COMPLETION_STATUS_TERMINAL_CODES
        .iter()
        .zip(generated_statuses)
    {
        assert_eq!(*contract_code as i32, generated_status as i32);
        assert_eq!(
            RpcCompletionStatus::from_terminal_code(*contract_code),
            Some(*contract_status)
        );
    }
    assert_eq!(GeneratedRpcCompletionStatus::Unspecified as i32, 0);
    assert!(RpcCompletionStatus::from_terminal_code(0).is_none());

    let generated_outcomes = [
        GeneratedTransactionOutcome::NotStarted,
        GeneratedTransactionOutcome::Committed,
        GeneratedTransactionOutcome::RolledBack,
        GeneratedTransactionOutcome::Failed,
        GeneratedTransactionOutcome::Cancelled,
    ];
    assert_eq!(
        generated_outcomes.len(),
        TRANSACTION_OUTCOME_TERMINAL_CODES.len()
    );
    for ((contract_outcome, contract_code), generated_outcome) in TRANSACTION_OUTCOME_TERMINAL_CODES
        .iter()
        .zip(generated_outcomes)
    {
        assert_eq!(*contract_code as i32, generated_outcome as i32);
        assert_eq!(
            TransactionOutcome::from_terminal_code(*contract_code),
            Some(*contract_outcome)
        );
    }
    assert_eq!(GeneratedTransactionOutcome::Unspecified as i32, 0);
    assert!(TransactionOutcome::from_terminal_code(0).is_none());
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

#[derive(Clone)]
struct GeneratedErrorEnvelopeCase {
    request_id: Option<u64>,
    session_id: Option<u64>,
    trace_id: Option<String>,
    family: i32,
    code: String,
    message: String,
    transaction_effect: i32,
    retry_disposition: i32,
    retry_after_ms: Option<u64>,
    backpressure: Option<GeneratedBackpressureCase>,
}

#[derive(Clone)]
struct GeneratedBackpressureCase {
    retry_after_ms: Option<u64>,
    capacity_percent: Option<u32>,
}

impl GeneratedErrorEnvelopeCase {
    fn valid_backpressure() -> Self {
        Self {
            request_id: Some(101),
            session_id: Some(202),
            trace_id: Some("trace-proto-101".to_string()),
            family: 4,
            code: "CONTRACT_HASH_MISMATCH".to_string(),
            message: "ContractHash mismatch".to_string(),
            transaction_effect: 1,
            retry_disposition: 4,
            retry_after_ms: None,
            backpressure: Some(GeneratedBackpressureCase {
                retry_after_ms: Some(50),
                capacity_percent: Some(70),
            }),
        }
    }
}

impl GeneratedErrorEnvelopeView for GeneratedErrorEnvelopeCase {
    type Backpressure = GeneratedBackpressureCase;

    fn request_id(&self) -> Option<u64> {
        self.request_id
    }

    fn session_id(&self) -> Option<u64> {
        self.session_id
    }

    fn trace_id(&self) -> Option<&str> {
        self.trace_id.as_deref()
    }

    fn family(&self) -> i32 {
        self.family
    }

    fn code(&self) -> &str {
        &self.code
    }

    fn message(&self) -> &str {
        &self.message
    }

    fn transaction_effect(&self) -> i32 {
        self.transaction_effect
    }

    fn retry_disposition(&self) -> i32 {
        self.retry_disposition
    }

    fn retry_after_ms(&self) -> Option<u64> {
        self.retry_after_ms
    }

    fn backpressure(&self) -> Option<&Self::Backpressure> {
        self.backpressure.as_ref()
    }
}

impl GeneratedBackpressureMetadataView for GeneratedBackpressureCase {
    fn retry_after_ms(&self) -> Option<u64> {
        self.retry_after_ms
    }

    fn capacity_percent(&self) -> Option<u32> {
        self.capacity_percent
    }
}

#[test]
fn generated_error_envelope_validation_stays_with_proto_wire() {
    let valid = GeneratedErrorEnvelopeCase::valid_backpressure();
    validate_generated_error_envelope(&valid).unwrap();

    let empty_code = GeneratedErrorEnvelopeCase {
        code: String::new(),
        ..valid.clone()
    };
    assert_eq!(
        validate_generated_error_envelope(&empty_code)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Contract
    );

    let over_capacity = GeneratedErrorEnvelopeCase {
        backpressure: Some(GeneratedBackpressureCase {
            capacity_percent: Some(101),
            ..valid.backpressure.clone().unwrap()
        }),
        ..valid.clone()
    };
    assert_eq!(
        validate_generated_error_envelope(&over_capacity)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Resource
    );

    let missing_backpressure = GeneratedErrorEnvelopeCase {
        backpressure: None,
        ..valid.clone()
    };
    assert_eq!(
        validate_generated_error_envelope(&missing_backpressure)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Resource
    );

    let retry_after_without_delay = GeneratedErrorEnvelopeCase {
        retry_disposition: 3,
        retry_after_ms: None,
        backpressure: None,
        ..valid
    };
    assert_eq!(
        validate_generated_error_envelope(&retry_after_without_delay)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Resource
    );
}
