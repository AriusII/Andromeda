use andromeda_core::{
    AndromedaErrorKind, CatalogVersion, ContractHash, RequestId, SessionId, TransactionId,
};
use andromeda_proto::{
    generated, BackpressureMetadata, ErrorEnvelope, ErrorFamily, FrameEnvelope, PayloadFrameFamily,
    PayloadKind, ProtocolVersion, ResultRowCountSummary, RetryDisposition, RpcCompletion,
    RpcCompletionStatus, RpcResultStreamMetadataPolicy, TransactionEffect, TransactionOutcome,
    AUTH_WIRE_CODE, CONTRACT_REQUEST_WIRE_CODE, CONTRACT_RESPONSE_WIRE_CODE, ERROR_WIRE_CODE,
    HELLO_WIRE_CODE, PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP, RPC_BATCH_WIRE_CODE,
    RPC_COMPLETION_WIRE_CODE, RPC_EXECUTE_REQUEST_WIRE_CODE, RPC_METADATA_WIRE_CODE,
};
use prost::Message;

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
fn generated_frame_envelope_has_stable_wire_projection() {
    let envelope = generated::protocol::v1::FrameEnvelope {
        protocol_version: Some(generated::protocol::v1::ProtocolVersion { major: 1, minor: 0 }),
        contract_hash: vec![7; ContractHash::LEN],
        catalog_version: 11,
        request_id: 101,
        session_id: 202,
        tx_id: Some(303),
        payload_kind: generated::protocol::v1::PayloadKind::RpcExecuteRequest as i32,
        payload: b"abc".to_vec(),
    };

    let encoded = envelope.encode_to_vec();
    let mut expected = vec![0x0a, 0x02, 0x08, 0x01, 0x12, 0x20];
    expected.extend_from_slice(&[7; ContractHash::LEN]);
    expected.extend_from_slice(&[
        0x18, 0x0b, 0x20, 0x65, 0x28, 0xca, 0x01, 0x30, 0xaf, 0x02, 0x38, 0x05, 0x42, 0x03, 0x61,
        0x62, 0x63,
    ]);

    assert_eq!(encoded, expected);

    let decoded = generated::protocol::v1::FrameEnvelope::decode(encoded.as_slice()).unwrap();
    assert_eq!(
        decoded.payload_kind,
        generated::protocol::v1::PayloadKind::RpcExecuteRequest as i32
    );
    assert_eq!(decoded.contract_hash, vec![7; ContractHash::LEN]);
    assert_eq!(decoded.request_id, 101);
    assert_eq!(decoded.session_id, 202);
    assert_eq!(decoded.tx_id, Some(303));
    assert_eq!(decoded.payload, b"abc");
}

#[test]
fn generated_rpc_completion_has_stable_wire_projection() {
    let completion = generated::protocol::v1::RpcCompletion {
        status: generated::protocol::v1::rpc_completion::Status::Committed as i32,
        rows_affected: Some(42),
        tx_id: Some(77),
        request_id: Some(101),
        session_id: Some(202),
        trace_id: Some("trace".to_string()),
        transaction_outcome: generated::protocol::v1::rpc_completion::TransactionOutcome::Committed
            as i32,
        durable_lsn: Some(7),
        result_row_counts: vec![
            generated::protocol::v1::rpc_completion::ResultRowCountSummary {
                result_name: "Reservation".to_string(),
                rows_emitted: 1,
                row_count_exact: Some(1),
            },
        ],
    };

    let encoded = completion.encode_to_vec();
    let expected = [
        0x08, 0x01, 0x10, 0x2a, 0x18, 0x4d, 0x80, 0x02, 0x65, 0x88, 0x02, 0xca, 0x01, 0x92, 0x02,
        0x05, 0x74, 0x72, 0x61, 0x63, 0x65, 0x98, 0x02, 0x02, 0xa0, 0x02, 0x07, 0xaa, 0x02, 0x11,
        0x0a, 0x0b, 0x52, 0x65, 0x73, 0x65, 0x72, 0x76, 0x61, 0x74, 0x69, 0x6f, 0x6e, 0x10, 0x01,
        0x18, 0x01,
    ];

    assert_eq!(encoded, expected);

    let decoded = generated::protocol::v1::RpcCompletion::decode(encoded.as_slice()).unwrap();
    assert_eq!(
        decoded.status,
        generated::protocol::v1::rpc_completion::Status::Committed as i32
    );
    assert_eq!(decoded.rows_affected, Some(42));
    assert_eq!(decoded.tx_id, Some(77));
    assert_eq!(decoded.durable_lsn, Some(7));
    assert_eq!(decoded.result_row_counts.len(), 1);
    assert_eq!(decoded.result_row_counts[0].row_count_exact, Some(1));
}

#[test]
fn generated_rpc_metadata_has_stable_wire_projection() {
    let metadata = generated::protocol::v1::RpcMetadata {
        result_streams: vec![generated::contract::v1::ResultStreamDescriptor {
            stream_name: "Reservation".to_string(),
            columns: vec![generated::contract::v1::ColumnDescriptor {
                name: "Reserved".to_string(),
                ordinal: 1,
                type_name: "bool".to_string(),
            }],
            cardinality: generated::contract::v1::result_stream_descriptor::Cardinality::ExactlyOne
                as i32,
            row_count_requirement:
            generated::contract::v1::result_stream_descriptor::RowCountRequirement::ExactRequired
                as i32,
            row_count_exact: Some(1),
            row_count_max: None,
        }],
        completion_policy: Some(generated::protocol::v1::ResultCompletionPolicy {
            completion_shape:
                generated::protocol::v1::result_completion_policy::CompletionShape::RequiresRowBatch
                    as i32,
            reason: "requires batch".to_string(),
        }),
    };

    let encoded = metadata.encode_to_vec();
    let expected = [
        0x0a, 0x27, 0x0a, 0x0b, 0x52, 0x65, 0x73, 0x65, 0x72, 0x76, 0x61, 0x74, 0x69, 0x6f, 0x6e,
        0x12, 0x12, 0x0a, 0x08, 0x52, 0x65, 0x73, 0x65, 0x72, 0x76, 0x65, 0x64, 0x10, 0x01, 0x1a,
        0x04, 0x62, 0x6f, 0x6f, 0x6c, 0x18, 0x04, 0x20, 0x03, 0x28, 0x01, 0x12, 0x12, 0x08, 0x01,
        0x12, 0x0e, 0x72, 0x65, 0x71, 0x75, 0x69, 0x72, 0x65, 0x73, 0x20, 0x62, 0x61, 0x74, 0x63,
        0x68,
    ];

    assert_eq!(encoded, expected);

    let decoded = generated::protocol::v1::RpcMetadata::decode(encoded.as_slice()).unwrap();
    assert_eq!(decoded.result_streams.len(), 1);
    assert_eq!(decoded.result_streams[0].stream_name, "Reservation");
    assert_eq!(decoded.result_streams[0].columns[0].type_name, "bool");
    assert_eq!(
        decoded.completion_policy.unwrap().completion_shape,
        generated::protocol::v1::result_completion_policy::CompletionShape::RequiresRowBatch as i32
    );
}

#[test]
fn generated_rpc_execute_request_and_batch_are_binary_projections() {
    let execute = generated::protocol::v1::RpcExecuteRequest {
        procedure_name: "Inventory.ReserveStock".to_string(),
        expected_contract_hash: vec![9; ContractHash::LEN],
        expected_catalog_version: 44,
        surface_scope: "application".to_string(),
        arguments: vec![generated::protocol::v1::rpc_execute_request::Argument {
            name: "Quantity".to_string(),
            type_name: "i64".to_string(),
            value: 3_i64.to_le_bytes().to_vec(),
        }],
        budget: Some(
            generated::protocol::v1::rpc_execute_request::RequestBudget {
                cpu_micros: Some(5_000),
                memory_bytes: Some(64 * 1024),
                io_bytes: Some(128 * 1024),
                priority_class: Some(1),
            },
        ),
    };

    let encoded_execute = execute.encode_to_vec();
    let decoded_execute =
        generated::protocol::v1::RpcExecuteRequest::decode(encoded_execute.as_slice()).unwrap();
    assert_eq!(decoded_execute.procedure_name, "Inventory.ReserveStock");
    assert_eq!(
        decoded_execute.expected_contract_hash,
        vec![9; ContractHash::LEN]
    );
    assert_eq!(decoded_execute.arguments[0].value, 3_i64.to_le_bytes());
    assert!(decoded_execute.budget.is_some());

    let batch = generated::protocol::v1::RpcBatch {
        result_name: "Inventory.ReserveStock.Reservation".to_string(),
        batch_index: 0,
        rows_emitted: 1,
        structured_payload: b"\x01".to_vec(),
        row_count_exact: Some(1),
        terminal_batch: true,
    };

    let encoded_batch = batch.encode_to_vec();
    let decoded_batch =
        generated::protocol::v1::RpcBatch::decode(encoded_batch.as_slice()).unwrap();
    assert_eq!(
        decoded_batch.result_name,
        "Inventory.ReserveStock.Reservation"
    );
    assert_eq!(decoded_batch.structured_payload, b"\x01");
    assert_eq!(decoded_batch.row_count_exact, Some(1));
    assert!(decoded_batch.terminal_batch);
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
        assert!(PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP
            .iter()
            .any(|(locked_kind, locked_code)| *locked_kind == kind && *locked_code == code));
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
fn generated_payload_kind_codes_match_custom_contract_codes() {
    use andromeda_proto::generated::protocol::v1::PayloadKind as GeneratedPayloadKind;

    let expected = [
        (GeneratedPayloadKind::Hello, HELLO_WIRE_CODE),
        (GeneratedPayloadKind::Auth, AUTH_WIRE_CODE),
        (
            GeneratedPayloadKind::ContractRequest,
            CONTRACT_REQUEST_WIRE_CODE,
        ),
        (
            GeneratedPayloadKind::ContractResponse,
            CONTRACT_RESPONSE_WIRE_CODE,
        ),
        (
            GeneratedPayloadKind::RpcExecuteRequest,
            RPC_EXECUTE_REQUEST_WIRE_CODE,
        ),
        (GeneratedPayloadKind::RpcMetadata, RPC_METADATA_WIRE_CODE),
        (GeneratedPayloadKind::RpcBatch, RPC_BATCH_WIRE_CODE),
        (
            GeneratedPayloadKind::RpcCompletion,
            RPC_COMPLETION_WIRE_CODE,
        ),
        (GeneratedPayloadKind::Error, ERROR_WIRE_CODE),
    ];

    for (generated_kind, code) in expected {
        assert_eq!(generated_kind as u32, code);
    }
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

    assert!(envelope(PayloadKind::RpcMetadata, Vec::new())
        .validate()
        .is_ok());
    assert!(envelope(PayloadKind::RpcCompletion, Vec::new())
        .validate()
        .is_ok());
}

#[test]
fn envelope_sequence_requires_metadata_batch_completion_in_one_context() {
    let metadata = envelope(PayloadKind::RpcMetadata, b"columns".to_vec());
    let batch = envelope(PayloadKind::RpcBatch, b"row".to_vec());
    let completion = envelope(PayloadKind::RpcCompletion, Vec::new());

    assert!(FrameEnvelope::validate_rpc_stream_sequence(&[
        metadata.clone(),
        batch.clone(),
        completion.clone(),
    ])
    .is_ok());

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
fn zero_row_or_mutation_only_completion_requires_explicit_metadata_policy() {
    let metadata = envelope(PayloadKind::RpcMetadata, b"zero-row-policy".to_vec());
    let completion = envelope(PayloadKind::RpcCompletion, Vec::new());
    let sequence = [metadata, completion];

    assert_eq!(
        FrameEnvelope::validate_rpc_stream_sequence(&sequence)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
    assert_eq!(
        FrameEnvelope::validate_rpc_stream_sequence_with_metadata_policy(
            &sequence,
            RpcResultStreamMetadataPolicy::RowBatchRequired,
        )
        .unwrap_err()
        .kind(),
        AndromedaErrorKind::Protocol
    );
    assert!(
        FrameEnvelope::validate_rpc_stream_sequence_with_metadata_policy(
            &sequence,
            RpcResultStreamMetadataPolicy::ZeroRowCompletionAllowed,
        )
        .is_ok()
    );
    assert!(
        FrameEnvelope::validate_rpc_stream_sequence_with_metadata_policy(
            &sequence,
            RpcResultStreamMetadataPolicy::MutationOnly,
        )
        .is_ok()
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
fn reserve_stock_completion_requires_exact_row_count_and_transaction_outcome_evidence() {
    let committed = RpcCompletion {
        request_id: Some(RequestId::new(701)),
        session_id: Some(SessionId::new(702)),
        trace_id: Some("reserve-stock-trace".to_string()),
        status: RpcCompletionStatus::Committed,
        transaction_outcome: TransactionOutcome::Committed,
        rows_affected: Some(2),
        result_row_counts: vec![ResultRowCountSummary {
            result_name: "Inventory.ReserveStock.Reservation".to_string(),
            rows_emitted: 1,
            row_count_exact: Some(1),
        }],
        tx_id: Some(TransactionId::new(703)),
        durable_lsn: Some(704),
    };
    assert!(committed.validate().is_ok());

    let wrong_exact_row_count = RpcCompletion {
        result_row_counts: vec![ResultRowCountSummary {
            result_name: "Inventory.ReserveStock.Reservation".to_string(),
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
