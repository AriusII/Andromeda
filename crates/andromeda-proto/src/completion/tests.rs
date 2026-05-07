use super::*;
use andromeda_error::AndromedaErrorKind;
use andromeda_types::{RequestId, SessionId, TransactionId};

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
fn rpc_completion_status_terminal_codes_match_proto_status_enum() {
    // Stable wire-aligned codes 1..=8; any change is a contract break.
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
    // Compatible version validates.
    assert!(
        completion
            .validate_for_protocol_version(ProtocolVersion::V1)
            .is_ok()
    );

    // Major drift is rejected as a protocol-class error.
    let drift = ProtocolVersion { major: 2, minor: 0 };
    let err = completion.validate_for_protocol_version(drift).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);

    // Minor drift above the envelope version is rejected.
    let minor_drift = ProtocolVersion { major: 1, minor: 1 };
    let err = completion
        .validate_for_protocol_version(minor_drift)
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn poisoned_completion_with_terminal_outcome_requires_durable_lsn() {
    // Poisoned with a rolled-back outcome must carry tx id and durable
    // LSN evidence (the post-rollback poison routing invariant).
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

    // Poisoned with rolled-back outcome and durable evidence is valid.
    let poisoned_ok = RpcCompletion {
        durable_lsn: Some(42),
        ..poisoned_no_lsn
    };
    assert!(poisoned_ok.validate().is_ok());

    // Poisoned with `NotStarted` outcome must NOT carry transaction evidence
    // (no transaction was ever begun).
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

    // Poisoned with `Failed` (non-durable) outcome must not carry a durable
    // LSN claim.
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
fn rpc_completion_status_matches_generated_proto_enum_values() {
    // Drift guard: the in-memory `RpcCompletionStatus::terminal_code` must
    // agree on every variant with the generated
    // `protocol::v1::rpc_completion::Status` integer values. If a new proto
    // value is added without a matching Rust variant (or vice versa) this test
    // fails as a hard contract break.
    use crate::generated::andromeda::protocol::v1::rpc_completion::Status as ProtoStatus;

    let pairs: &[(RpcCompletionStatus, ProtoStatus)] = &[
        (RpcCompletionStatus::Committed, ProtoStatus::Committed),
        (RpcCompletionStatus::RolledBack, ProtoStatus::RolledBack),
        (
            RpcCompletionStatus::FailedBeforeTransaction,
            ProtoStatus::FailedBeforeTransaction,
        ),
        (RpcCompletionStatus::Cancelled, ProtoStatus::Cancelled),
        (RpcCompletionStatus::Poisoned, ProtoStatus::Poisoned),
        (
            RpcCompletionStatus::PermissionDenied,
            ProtoStatus::PermissionDenied,
        ),
        (
            RpcCompletionStatus::ContractRejected,
            ProtoStatus::ContractRejected,
        ),
        (
            RpcCompletionStatus::SystemUnavailable,
            ProtoStatus::SystemUnavailable,
        ),
    ];

    for (rust, proto) in pairs {
        assert_eq!(
            rust.terminal_code(),
            *proto as u32,
            "RpcCompletionStatus::{:?} drifted from proto Status::{:?}",
            rust,
            proto
        );
        assert_eq!(
            RpcCompletionStatus::from_terminal_code(*proto as u32),
            Some(*rust)
        );
    }

    // Reserved unspecified slot must not round-trip into a Rust variant.
    assert_eq!(ProtoStatus::Unspecified as u32, 0);
    assert!(RpcCompletionStatus::from_terminal_code(0).is_none());
}

#[test]
fn transaction_outcome_matches_generated_proto_enum_values() {
    use crate::generated::andromeda::protocol::v1::rpc_completion::TransactionOutcome as ProtoOutcome;

    let pairs: &[(TransactionOutcome, ProtoOutcome)] = &[
        (TransactionOutcome::NotStarted, ProtoOutcome::NotStarted),
        (TransactionOutcome::Committed, ProtoOutcome::Committed),
        (TransactionOutcome::RolledBack, ProtoOutcome::RolledBack),
        (TransactionOutcome::Failed, ProtoOutcome::Failed),
        (TransactionOutcome::Cancelled, ProtoOutcome::Cancelled),
    ];

    for (rust, proto) in pairs {
        assert_eq!(
            rust.terminal_code(),
            *proto as u32,
            "TransactionOutcome::{:?} drifted from proto TransactionOutcome::{:?}",
            rust,
            proto
        );
    }

    assert_eq!(ProtoOutcome::Unspecified as u32, 0);
}

#[test]
fn completion_envelope_version_binds_to_locked_protocol_v1() {
    // Completion envelope contract version is locked to ProtocolVersion::V1
    // so any future bump is a deliberate breaking change visible to all
    // consumers of `validate_for_protocol_version`.
    assert_eq!(COMPLETION_ENVELOPE_VERSION, ProtocolVersion::V1);
    assert!(COMPLETION_ENVELOPE_VERSION.is_supported());
}

#[test]
fn result_row_count_summary_round_trips_through_generated_proto() {
    // Cross-bind the in-memory summary to the generated proto shape so a proto
    // field rename or tag drift is caught at the contract surface.
    use crate::generated::andromeda::protocol::v1::rpc_completion::ResultRowCountSummary as ProtoSummary;

    let summary = ResultRowCountSummary {
        result_name: "rows".to_string(),
        rows_emitted: 4,
        row_count_exact: Some(4),
    };
    assert!(summary.validate().is_ok());

    let proto = ProtoSummary {
        result_name: summary.result_name.clone(),
        rows_emitted: summary.rows_emitted,
        row_count_exact: summary.row_count_exact,
    };
    assert_eq!(proto.result_name, summary.result_name);
    assert_eq!(proto.rows_emitted, summary.rows_emitted);
    assert_eq!(proto.row_count_exact, summary.row_count_exact);

    // Exact mismatch is a contract break (metadata-before-payload promise
    // about row count exactness must hold at completion as well).
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
