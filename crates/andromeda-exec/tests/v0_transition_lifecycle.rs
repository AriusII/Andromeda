//! V0 cross-engine execution transition trace contract.
//!
//! Asserts that `InvocationCompletion::project_transition` and
//! `InvocationReject::project_transition` both produce
//! `ExecutionTransitionTrace` evidence with stable correlation fields, and
//! that pre-transaction rejection paths cannot fabricate transaction or
//! durable LSN evidence.

use andromeda_core::{InvocationId, RequestId, SessionId, TransactionId};
use andromeda_exec::{CompletionMappingService, CompletionStatus, InvocationReject};
use andromeda_observability::{TraceId, TransactionPhaseCode, TransitionReasonCode};
use andromeda_transaction::TransactionState;
use andromeda_wal::Lsn;

#[test]
fn committed_invocation_projects_terminal_transition_with_durable_lsn() {
    let completion = CompletionMappingService::committed(
        InvocationId::new(11),
        2,
        TransactionState::Committed,
        Lsn::new(900),
        TraceId::new(7),
    )
    .expect("committed completion has durable WAL evidence");

    let trace = completion.project_transition(
        Some(TransactionState::Committing),
        Some(RequestId::new(3)),
        Some(SessionId::new(4)),
        Some(TransactionId::new(101)),
        "commit visible",
    );

    assert_eq!(trace.invocation_id, InvocationId::new(11));
    assert_eq!(trace.request_id, Some(RequestId::new(3)));
    assert_eq!(trace.session_id, Some(SessionId::new(4)));
    assert_eq!(trace.transaction_id, Some(TransactionId::new(101)));
    assert_eq!(
        trace.completion_code,
        Some(CompletionStatus::Committed.terminal_code())
    );
    assert_eq!(trace.prev_phase, Some(TransactionPhaseCode::COMMITTING));
    assert_eq!(trace.next_phase, Some(TransactionPhaseCode::COMMITTED));
    assert_eq!(trace.durable_lsn, Some(900));
    assert_eq!(trace.reason_code, TransitionReasonCode::DURABLE_WAL_FLUSH);
    assert!(trace.proves_terminal_evidence());
    assert!(trace.validate().is_ok());
}

#[test]
fn permission_denied_completion_strips_transaction_and_lsn_correlation() {
    // Even if the caller mistakenly passes transaction evidence, the
    // projection must defensively drop it for pre-transaction rejection
    // statuses.
    let completion = CompletionMappingService::rejected(
        InvocationId::new(22),
        CompletionStatus::PermissionDenied,
        TraceId::new(8),
    )
    .expect("permission denied completion has no transaction evidence");

    let trace = completion.project_transition(
        None,
        Some(RequestId::new(5)),
        Some(SessionId::new(6)),
        Some(TransactionId::new(202)),
        "permission denied",
    );

    assert!(trace.transaction_id.is_none());
    assert!(trace.durable_lsn.is_none());
    assert_eq!(trace.reason_code, TransitionReasonCode::PERMISSION_DENIED);
    assert!(trace.validate().is_ok());
}

#[test]
fn invocation_reject_projects_pre_transaction_transition_without_evidence() {
    let reject = InvocationReject {
        status: CompletionStatus::ContractRejected,
        reason: "contract hash mismatch".to_string(),
    };

    let trace = reject.project_transition(
        InvocationId::new(33),
        TraceId::new(9),
        Some(RequestId::new(7)),
        Some(SessionId::new(8)),
    );

    assert_eq!(trace.invocation_id, InvocationId::new(33));
    assert!(trace.transaction_id.is_none());
    assert!(trace.durable_lsn.is_none());
    assert!(trace.prev_phase.is_none());
    assert!(trace.next_phase.is_none());
    assert_eq!(
        trace.reason_code,
        TransitionReasonCode::PRE_TRANSACTION_REJECTION
    );
    assert_eq!(
        trace.completion_code,
        Some(CompletionStatus::ContractRejected.terminal_code())
    );
    assert!(trace.validate().is_ok());

    // The lossy DecisionTrace projection routes contract rejection into the
    // ContractRejected critical decision kind so existing sinks can ingest
    // it without a transition-aware envelope.
    let decision = trace.as_decision_trace();
    assert_eq!(decision.trace_id, TraceId::new(9));
    assert!(decision.has_explanation());
}

#[test]
fn rolled_back_completion_requires_durable_rollback_lsn() {
    let err = CompletionMappingService::rolled_back(
        InvocationId::new(44),
        TransactionState::RolledBack,
        Lsn::ZERO,
        TraceId::new(10),
    )
    .expect_err("rolled-back completion requires durable WAL evidence");

    assert!(err.message().contains("durable WAL LSN evidence"));
}

#[test]
fn execution_transition_completion_codes_match_proto() {
    use andromeda_proto::RpcCompletionStatus as Proto;
    let pairs: &[(CompletionStatus, Proto)] = &[
        (CompletionStatus::Committed, Proto::Committed),
        (CompletionStatus::RolledBack, Proto::RolledBack),
        (
            CompletionStatus::FailedBeforeTransaction,
            Proto::FailedBeforeTransaction,
        ),
        (CompletionStatus::Cancelled, Proto::Cancelled),
        (CompletionStatus::Poisoned, Proto::Poisoned),
        (CompletionStatus::PermissionDenied, Proto::PermissionDenied),
        (CompletionStatus::ContractRejected, Proto::ContractRejected),
        (
            CompletionStatus::SystemUnavailable,
            Proto::SystemUnavailable,
        ),
    ];
    for (exec, proto) in pairs {
        assert_eq!(exec.terminal_code(), proto.terminal_code());
    }
}
