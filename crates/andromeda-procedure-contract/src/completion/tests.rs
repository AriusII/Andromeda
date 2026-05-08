use super::*;
use andromeda_error::AndromedaErrorKind;
use andromeda_types::{RequestId, SessionId, TransactionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TestProtocolVersion {
    major: u32,
    minor: u32,
    valid: bool,
}

impl CompletionProtocolVersion for TestProtocolVersion {
    fn validate_completion_protocol_version(self) -> andromeda_error::AndromedaResult<()> {
        if self.valid {
            Ok(())
        } else {
            Err(andromeda_error::AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "test protocol version rejected",
            ))
        }
    }

    fn completion_protocol_major(self) -> u32 {
        self.major
    }

    fn completion_protocol_minor(self) -> u32 {
        self.minor
    }
}

fn committed_template() -> RpcCompletion {
    RpcCompletion {
        request_id: Some(RequestId::new(1)),
        session_id: Some(SessionId::new(2)),
        trace_id: Some("trace".to_string()),
        status: RpcCompletionStatus::Committed,
        transaction_outcome: TransactionOutcome::Committed,
        rows_affected: Some(1),
        result_row_counts: Vec::new(),
        tx_id: Some(TransactionId::new(99)),
        durable_lsn: Some(7),
    }
}

#[test]
fn rpc_completion_status_terminal_codes_are_stable() {
    assert_eq!(RpcCompletionStatus::Committed.terminal_code(), 1);
    assert_eq!(RpcCompletionStatus::RolledBack.terminal_code(), 2);
    assert_eq!(
        RpcCompletionStatus::FailedBeforeTransaction.terminal_code(),
        3
    );
    assert_eq!(RpcCompletionStatus::Cancelled.terminal_code(), 4);
    assert_eq!(RpcCompletionStatus::Poisoned.terminal_code(), 5);
    assert_eq!(RpcCompletionStatus::PermissionDenied.terminal_code(), 6);
    assert_eq!(RpcCompletionStatus::ContractRejected.terminal_code(), 7);
    assert_eq!(RpcCompletionStatus::SystemUnavailable.terminal_code(), 8);

    for code in 1u32..=8 {
        let status = RpcCompletionStatus::from_terminal_code(code).unwrap();
        assert_eq!(status.terminal_code(), code);
    }
    assert!(RpcCompletionStatus::from_terminal_code(0).is_none());
    assert!(RpcCompletionStatus::from_terminal_code(9).is_none());

    assert!(RpcCompletionStatus::Committed.is_transactional_terminal());
    assert!(RpcCompletionStatus::RolledBack.is_transactional_terminal());
    assert!(!RpcCompletionStatus::Poisoned.is_transactional_terminal());
}

#[test]
fn transaction_outcome_terminal_codes_are_stable() {
    assert_eq!(TransactionOutcome::NotStarted.terminal_code(), 1);
    assert_eq!(TransactionOutcome::Committed.terminal_code(), 2);
    assert_eq!(TransactionOutcome::RolledBack.terminal_code(), 3);
    assert_eq!(TransactionOutcome::Failed.terminal_code(), 4);
    assert_eq!(TransactionOutcome::Cancelled.terminal_code(), 5);
}

#[test]
fn rpc_completion_validates_against_negotiated_protocol_version() {
    let completion = committed_template();
    assert!(
        completion
            .validate_for_protocol_version(TestProtocolVersion {
                major: 1,
                minor: 0,
                valid: true,
            })
            .is_ok()
    );

    let drift = TestProtocolVersion {
        major: 2,
        minor: 0,
        valid: true,
    };
    let err = completion.validate_for_protocol_version(drift).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);

    let invalid = TestProtocolVersion {
        major: 1,
        minor: 1,
        valid: false,
    };
    let err = completion
        .validate_for_protocol_version(invalid)
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn poisoned_completion_with_terminal_outcome_requires_durable_lsn() {
    let poisoned_no_lsn = RpcCompletion {
        status: RpcCompletionStatus::Poisoned,
        transaction_outcome: TransactionOutcome::RolledBack,
        rows_affected: Some(0),
        durable_lsn: None,
        ..committed_template()
    };
    let err = poisoned_no_lsn.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Storage);

    let poisoned_zero_lsn = RpcCompletion {
        durable_lsn: Some(0),
        ..poisoned_no_lsn.clone()
    };
    let err = poisoned_zero_lsn.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Storage);

    let poisoned_ok = RpcCompletion {
        durable_lsn: Some(42),
        ..poisoned_no_lsn
    };
    assert!(poisoned_ok.validate().is_ok());

    let poisoned_not_started_with_tx = RpcCompletion {
        status: RpcCompletionStatus::Poisoned,
        transaction_outcome: TransactionOutcome::NotStarted,
        rows_affected: None,
        tx_id: Some(TransactionId::new(99)),
        durable_lsn: None,
        ..committed_template()
    };
    let err = poisoned_not_started_with_tx.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);

    let poisoned_failed_with_lsn = RpcCompletion {
        status: RpcCompletionStatus::Poisoned,
        transaction_outcome: TransactionOutcome::Failed,
        rows_affected: None,
        tx_id: None,
        durable_lsn: Some(7),
        ..committed_template()
    };
    let err = poisoned_failed_with_lsn.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
}

#[test]
fn completion_envelope_version_binds_to_locked_protocol_v1() {
    assert_eq!(
        COMPLETION_ENVELOPE_CONTRACT_VERSION,
        CompletionEnvelopeVersion::V1
    );
    assert!(COMPLETION_ENVELOPE_CONTRACT_VERSION.is_compatible_with_protocol(1, 0));
    assert!(!COMPLETION_ENVELOPE_CONTRACT_VERSION.is_compatible_with_protocol(1, 1));
    assert!(!COMPLETION_ENVELOPE_CONTRACT_VERSION.is_compatible_with_protocol(2, 0));
}

#[test]
fn result_row_count_summary_rejects_exact_mismatch() {
    let summary = ResultRowCountSummary {
        result_name: "rows".to_string(),
        rows_emitted: 4,
        row_count_exact: Some(4),
    };
    assert!(summary.validate().is_ok());

    let bad = ResultRowCountSummary {
        result_name: "rows".to_string(),
        rows_emitted: 3,
        row_count_exact: Some(4),
    };
    assert_eq!(
        bad.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );
}
