use andromeda_core::{
    AndromedaErrorKind, CatalogVersion, ContractHash, RequestId, SessionId, TransactionId,
};
use andromeda_proto::{
    BackpressureMetadata, ErrorEnvelope, ErrorFamily, FrameEnvelope,
    PayloadFrameFamily, PayloadKind, ProtocolVersion, ResultRowCountSummary, RetryDisposition,
    RpcCompletion, RpcCompletionStatus, TransactionEffect, TransactionOutcome,
    AUTH_WIRE_CODE, CONTRACT_REQUEST_WIRE_CODE, CONTRACT_RESPONSE_WIRE_CODE,
    ERROR_WIRE_CODE, HELLO_WIRE_CODE, PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP, RPC_BATCH_WIRE_CODE,
    RPC_COMPLETION_WIRE_CODE, RPC_EXECUTE_REQUEST_WIRE_CODE, RPC_METADATA_WIRE_CODE,
};

fn hash(byte: u8) -> ContractHash {
    ContractHash::test_vector(byte)
}

fn envelope(payload_kind: PayloadKind, payload: impl Into<Vec<u8>>) -> FrameEnvelope {
    FrameEnvelope {
        protocol_version: ProtocolVersion::V1,
        contract_hash: hash(7),
        catalog_version: CatalogVersion::new(11),
        request_id: RequestId::new(101),
        session_id: SessionId::new(202),
        tx_id: Some(TransactionId::new(303)),
        payload_kind,
        payload: payload.into(),
    }
}

#[test]
fn payload_kind_wire_codes_are_contract_locked() {
    let expected = [
        (
            PayloadKind::Hello,
            HELLO_WIRE_CODE,
            PayloadFrameFamily::SessionControl,
        ),
        (
            PayloadKind::Auth,
            AUTH_WIRE_CODE,
            PayloadFrameFamily::SessionControl,
        ),
        (
            PayloadKind::ContractRequest,
            CONTRACT_REQUEST_WIRE_CODE,
            PayloadFrameFamily::ContractControl,
        ),
        (
            PayloadKind::ContractResponse,
            CONTRACT_RESPONSE_WIRE_CODE,
            PayloadFrameFamily::ContractControl,
        ),
        (
            PayloadKind::RpcExecuteRequest,
            RPC_EXECUTE_REQUEST_WIRE_CODE,
            PayloadFrameFamily::RpcCommand,
        ),
        (
            PayloadKind::RpcMetadata,
            RPC_METADATA_WIRE_CODE,
            PayloadFrameFamily::RpcResultStream,
        ),
        (
            PayloadKind::RpcBatch,
            RPC_BATCH_WIRE_CODE,
            PayloadFrameFamily::RpcResultStream,
        ),
        (
            PayloadKind::RpcCompletion,
            RPC_COMPLETION_WIRE_CODE,
            PayloadFrameFamily::RpcResultStream,
        ),
        (
            PayloadKind::Error,
            ERROR_WIRE_CODE,
            PayloadFrameFamily::Diagnostic,
        ),
    ];

    assert_eq!(PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP.len(), expected.len());

    for (kind, code, family) in expected {
        assert_eq!(kind.wire_code(), code);
        assert_eq!(PayloadKind::try_from(code).unwrap(), kind);
        assert_eq!(kind.frame_mapping().transport_frame_code, code);
        assert_eq!(kind.frame_mapping().family, family);
        assert!(
            PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP
                .iter()
                .any(|(locked_kind, locked_code)| *locked_kind == kind && *locked_code == code)
        );
    }

    assert_eq!(
        PayloadKind::try_from(0).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
    assert_eq!(
        PayloadKind::try_from(10).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
    assert_eq!(
        PayloadKind::try_from(100).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn contract_bound_envelopes_require_nonzero_contract_hash() {
    for kind in [
        PayloadKind::RpcExecuteRequest,
        PayloadKind::RpcMetadata,
        PayloadKind::RpcBatch,
        PayloadKind::RpcCompletion,
    ] {
        assert!(kind.requires_contract_hash());

        let payload = if kind.requires_non_empty_payload() {
            b"payload".to_vec()
        } else {
            Vec::new()
        };
        let invalid = FrameEnvelope {
            contract_hash: ContractHash::zero(),
            payload_kind: kind,
            payload,
            ..envelope(kind, Vec::new())
        };

        assert_eq!(
            invalid.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }

    for kind in [
        PayloadKind::Hello,
        PayloadKind::Auth,
        PayloadKind::ContractRequest,
        PayloadKind::ContractResponse,
        PayloadKind::Error,
    ] {
        let contractless = FrameEnvelope {
            contract_hash: ContractHash::zero(),
            payload_kind: kind,
            payload: Vec::new(),
            ..envelope(kind, Vec::new())
        };

        assert!(!kind.requires_contract_hash());
        assert!(
            contractless.validate().is_ok(),
            "{kind:?} should not require a contract hash"
        );
    }
}

#[test]
fn rpc_execute_and_batch_payload_bodies_are_required() {
    let execute = FrameEnvelope::rpc_execute_request(
        hash(9),
        CatalogVersion::new(12),
        RequestId::new(404),
        SessionId::new(505),
        None,
        Vec::new(),
    );
    assert_eq!(execute.unwrap_err().kind(), AndromedaErrorKind::Protocol);

    let batch = envelope(PayloadKind::RpcBatch, Vec::new());
    assert_eq!(
        batch.validate().unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );

    assert!(
        envelope(PayloadKind::RpcMetadata, Vec::new())
            .validate()
            .is_ok()
    );
    assert!(
        envelope(PayloadKind::RpcCompletion, Vec::new())
            .validate()
            .is_ok()
    );
}

#[test]
fn envelope_sequence_requires_metadata_batch_completion_in_one_context() {
    let metadata = envelope(PayloadKind::RpcMetadata, b"columns".to_vec());
    let batch = envelope(PayloadKind::RpcBatch, b"row".to_vec());
    let completion = envelope(PayloadKind::RpcCompletion, Vec::new());

    assert!(
        FrameEnvelope::validate_rpc_stream_sequence(&[
            metadata.clone(),
            batch.clone(),
            completion.clone(),
        ])
            .is_ok()
    );

    assert_eq!(
        FrameEnvelope::validate_rpc_stream_sequence(&[metadata.clone(), completion.clone()])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
    assert_eq!(
        FrameEnvelope::validate_rpc_stream_sequence(&[batch.clone(), metadata.clone()])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );

    let mut wrong_context = batch.clone();
    wrong_context.session_id = SessionId::new(999);
    assert_eq!(
        FrameEnvelope::validate_rpc_stream_sequence(&[metadata.clone(), wrong_context])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );

    let wrong_kind = envelope(PayloadKind::Error, b"diagnostic".to_vec());
    assert_eq!(
        FrameEnvelope::validate_rpc_stream_sequence(&[metadata, wrong_kind])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
}

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
            result_name: "Reservation".to_string(),
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
