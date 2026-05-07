use andromeda_core::{
    AndromedaErrorKind, CatalogVersion, ContractHash, RequestId, SessionId, TransactionId,
};
use andromeda_proto::{
    BackpressureMetadata, ErrorEnvelope, ErrorFamily, FrameEnvelope as ProtoFrameEnvelope,
    PayloadKind, ProtocolVersion, RPC_EXECUTE_REQUEST_WIRE_CODE, RetryDisposition,
    TransactionEffect, decode_generated_message, encode_generated_message, generated,
};
use andromeda_quic::{
    BackpressureReason, BackpressureSignal, DispatchPolicy, FRAME_CODEC_CRC_OFFSET,
    FRAME_CODEC_HEADER_LEN, FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameCodec, FrameHeader,
    FrameType, ResultStreamMetadataPolicy, StreamRole, TypedResultStreamBounds,
    TypedResultStreamContext, decode_typed_frame_envelope, validate_result_stream_sequence,
    validate_single_frame_on_stream, validate_typed_result_stream_sequence,
    validate_typed_result_stream_sequence_with_context_and_bounds,
};

fn hash(byte: u8) -> Vec<u8> {
    vec![byte; ContractHash::LEN]
}

fn header(frame_type: FrameType, payload_length: u64) -> FrameHeader {
    FrameHeader {
        frame_type,
        request_id: RequestId::new(501),
        session_id: SessionId::new(601),
        tx_id: Some(TransactionId::new(701)),
        payload_length,
        flags: 0,
        header_crc: FRAME_HEADER_CRC_UNCHECKED,
    }
}

fn frame(frame_type: FrameType, payload: Vec<u8>) -> FrameBytes {
    FrameBytes {
        header: header(frame_type, payload.len() as u64),
        payload,
    }
}

fn envelope_payload(kind: generated::protocol::v1::PayloadKind, payload: Vec<u8>) -> Vec<u8> {
    let envelope = generated::protocol::v1::FrameEnvelope {
        protocol_version: Some(generated::protocol::v1::ProtocolVersion { major: 1, minor: 0 }),
        contract_hash: hash(7),
        catalog_version: 42,
        request_id: 501,
        session_id: 601,
        tx_id: Some(701),
        payload_kind: kind as i32,
        payload,
    };
    encode_generated_message(&envelope)
}

fn typed_result_stream_context() -> TypedResultStreamContext {
    TypedResultStreamContext::new(
        RequestId::new(501),
        SessionId::new(601),
        Some(TransactionId::new(701)),
        ContractHash::from_slice(&hash(7)).unwrap(),
        CatalogVersion::new(42),
    )
}

fn typed_result_stream_frames() -> [FrameBytes; 3] {
    let metadata_payload = encode_generated_message(&generated::protocol::v1::RpcMetadata {
        result_streams: Vec::new(),
        completion_policy: Some(generated::protocol::v1::ResultCompletionPolicy {
            completion_shape:
                generated::protocol::v1::result_completion_policy::CompletionShape::RequiresRowBatch
                    as i32,
            reason: "requires batch".to_string(),
        }),
    });
    let batch_payload = encode_generated_message(&generated::protocol::v1::RpcBatch {
        result_name: "Inventory.ReserveStock.Reservation".to_string(),
        batch_index: 0,
        rows_emitted: 1,
        structured_payload: b"\x01".to_vec(),
        row_count_exact: Some(1),
        terminal_batch: true,
    });
    let completion_payload = encode_generated_message(&generated::protocol::v1::RpcCompletion {
        status: generated::protocol::v1::rpc_completion::Status::Committed as i32,
        rows_affected: Some(2),
        tx_id: Some(701),
        request_id: Some(501),
        session_id: Some(601),
        trace_id: Some("trace".to_string()),
        transaction_outcome: generated::protocol::v1::rpc_completion::TransactionOutcome::Committed
            as i32,
        durable_lsn: Some(3),
        result_row_counts: Vec::new(),
    });

    [
        frame(
            FrameType::RpcMetadata,
            envelope_payload(
                generated::protocol::v1::PayloadKind::RpcMetadata,
                metadata_payload,
            ),
        ),
        frame(
            FrameType::RpcBatch,
            envelope_payload(
                generated::protocol::v1::PayloadKind::RpcBatch,
                batch_payload,
            ),
        ),
        frame(
            FrameType::RpcCompletion,
            envelope_payload(
                generated::protocol::v1::PayloadKind::RpcCompletion,
                completion_payload,
            ),
        ),
    ]
}

fn proto_envelope_from_generated(
    envelope: generated::protocol::v1::FrameEnvelope,
) -> ProtoFrameEnvelope {
    let protocol_version = envelope.protocol_version.unwrap();

    ProtoFrameEnvelope {
        protocol_version: ProtocolVersion {
            major: protocol_version.major,
            minor: protocol_version.minor,
        },
        contract_hash: ContractHash::from_slice(&envelope.contract_hash).unwrap(),
        catalog_version: CatalogVersion::new(envelope.catalog_version),
        request_id: RequestId::new(envelope.request_id),
        session_id: SessionId::new(envelope.session_id),
        tx_id: envelope.tx_id.map(TransactionId::new),
        payload_kind: PayloadKind::try_from(envelope.payload_kind as u32).unwrap(),
        payload: envelope.payload,
    }
}

#[test]
fn frame_codec_carries_generated_rpc_execute_request_without_translation() {
    let request = generated::protocol::v1::RpcExecuteRequest {
        procedure_name: "Inventory.ReserveStock".to_string(),
        expected_contract_hash: hash(7),
        expected_catalog_version: 42,
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
        expected_stats_version: Some(3),
    };
    let payload = envelope_payload(
        generated::protocol::v1::PayloadKind::RpcExecuteRequest,
        encode_generated_message(&request),
    );
    let encoded = FrameCodec::encode(&frame(FrameType::RpcExecuteRequest, payload)).unwrap();

    assert_eq!(&encoded[4..8], &RPC_EXECUTE_REQUEST_WIRE_CODE.to_be_bytes());
    assert!(encoded.len() > FRAME_CODEC_HEADER_LEN);

    let decoded_frame = FrameCodec::decode(&encoded).unwrap();
    validate_single_frame_on_stream(&decoded_frame, StreamRole::CommandBidirectional).unwrap();
    let decoded_envelope: generated::protocol::v1::FrameEnvelope =
        decode_generated_message(decoded_frame.payload.as_slice()).unwrap();
    let proto_envelope = proto_envelope_from_generated(decoded_envelope.clone());
    proto_envelope.validate().unwrap();
    let decoded_request: generated::protocol::v1::RpcExecuteRequest =
        decode_generated_message(decoded_envelope.payload.as_slice()).unwrap();

    assert_eq!(
        decoded_frame.header.frame_type,
        FrameType::RpcExecuteRequest
    );
    assert_eq!(
        decoded_envelope.payload_kind as u32,
        decoded_frame.header.frame_type.wire_code()
    );
    assert_eq!(decoded_request.procedure_name, "Inventory.ReserveStock");
    assert_eq!(decoded_request.expected_contract_hash, hash(7));
    assert_eq!(decoded_request.expected_stats_version, Some(3));
    assert_eq!(decoded_request.arguments[0].value, 3_i64.to_le_bytes());
}

#[test]
fn result_stream_frames_carry_generated_metadata_batch_completion_payloads() {
    let metadata_payload = generated::protocol::v1::RpcMetadata {
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
    let metadata_payload = encode_generated_message(&metadata_payload);
    let batch_payload = generated::protocol::v1::RpcBatch {
        result_name: "Inventory.ReserveStock.Reservation".to_string(),
        batch_index: 0,
        rows_emitted: 1,
        structured_payload: b"\x01".to_vec(),
        row_count_exact: Some(1),
        terminal_batch: true,
    };
    let batch_payload = encode_generated_message(&batch_payload);
    let completion_payload = generated::protocol::v1::RpcCompletion {
        status: generated::protocol::v1::rpc_completion::Status::Committed as i32,
        rows_affected: Some(2),
        tx_id: Some(701),
        request_id: Some(501),
        session_id: Some(601),
        trace_id: Some("trace".to_string()),
        transaction_outcome: generated::protocol::v1::rpc_completion::TransactionOutcome::Committed
            as i32,
        durable_lsn: Some(3),
        result_row_counts: vec![
            generated::protocol::v1::rpc_completion::ResultRowCountSummary {
                result_name: "Inventory.ReserveStock.Reservation".to_string(),
                rows_emitted: 1,
                row_count_exact: Some(1),
            },
        ],
    };
    let completion_payload = encode_generated_message(&completion_payload);

    let frames = [
        frame(
            FrameType::RpcMetadata,
            envelope_payload(
                generated::protocol::v1::PayloadKind::RpcMetadata,
                metadata_payload,
            ),
        ),
        frame(
            FrameType::RpcBatch,
            envelope_payload(
                generated::protocol::v1::PayloadKind::RpcBatch,
                batch_payload,
            ),
        ),
        frame(
            FrameType::RpcCompletion,
            envelope_payload(
                generated::protocol::v1::PayloadKind::RpcCompletion,
                completion_payload,
            ),
        ),
    ];

    validate_result_stream_sequence(&frames).unwrap();
    let mut dispatch = DispatchPolicy::new_result_stream(typed_result_stream_context());
    let mut proto_envelopes = Vec::new();

    for frame in &frames {
        dispatch.dispatch(frame).unwrap();
        let encoded = FrameCodec::encode(frame).unwrap();
        assert_eq!(
            &encoded[4..8],
            &frame.header.frame_type.wire_code().to_be_bytes()
        );

        let decoded_frame = FrameCodec::decode(&encoded).unwrap();
        let decoded_envelope: generated::protocol::v1::FrameEnvelope =
            decode_generated_message(decoded_frame.payload.as_slice()).unwrap();
        let proto_envelope = proto_envelope_from_generated(decoded_envelope.clone());
        proto_envelope.validate().unwrap();

        assert_eq!(
            decoded_envelope.payload_kind as u32,
            decoded_frame.header.frame_type.wire_code()
        );
        assert!(!decoded_envelope.payload.is_empty());
        proto_envelopes.push(proto_envelope);
    }

    dispatch.finish().unwrap();
    ProtoFrameEnvelope::validate_rpc_stream_sequence(&proto_envelopes).unwrap();
    validate_typed_result_stream_sequence(&frames).unwrap();

    let decoded_metadata: generated::protocol::v1::RpcMetadata =
        decode_generated_message(proto_envelopes[0].payload.as_slice()).unwrap();
    let decoded_batch: generated::protocol::v1::RpcBatch =
        decode_generated_message(proto_envelopes[1].payload.as_slice()).unwrap();
    let decoded_completion: generated::protocol::v1::RpcCompletion =
        decode_generated_message(proto_envelopes[2].payload.as_slice()).unwrap();

    assert_eq!(
        decoded_metadata.result_streams[0].row_count_exact,
        decoded_batch.row_count_exact
    );
    assert_eq!(
        decoded_completion.result_row_counts[0].row_count_exact,
        decoded_batch.row_count_exact
    );
}

#[test]
fn dispatch_policy_requires_admitted_typed_result_stream_context() {
    let frames = typed_result_stream_frames();
    let mut missing_context = DispatchPolicy::new(StreamRole::ResultUnidirectional);
    let missing = missing_context.dispatch(&frames[0]).unwrap_err();
    assert_eq!(missing.kind(), AndromedaErrorKind::Protocol);
    assert!(
        missing
            .message()
            .contains("admitted typed ResultStream context"),
        "result-stream dispatch must reject policies created without route context"
    );

    let wrong_contract_context = TypedResultStreamContext::new(
        RequestId::new(501),
        SessionId::new(601),
        Some(TransactionId::new(701)),
        ContractHash::from_slice(&hash(8)).unwrap(),
        CatalogVersion::new(42),
    );
    let mut wrong_context = DispatchPolicy::new_result_stream(wrong_contract_context);
    let mismatch = wrong_context.dispatch(&frames[0]).unwrap_err();
    assert_eq!(mismatch.kind(), AndromedaErrorKind::Protocol);
    assert!(
        mismatch.message().contains("expected route context"),
        "result-stream dispatch must reject ContractHash/CatalogVersion route drift"
    );

    let wrong_catalog_context = TypedResultStreamContext::new(
        RequestId::new(501),
        SessionId::new(601),
        Some(TransactionId::new(701)),
        ContractHash::from_slice(&hash(7)).unwrap(),
        CatalogVersion::new(43),
    );
    let mut wrong_catalog = DispatchPolicy::new_result_stream(wrong_catalog_context);
    let catalog_mismatch = wrong_catalog.dispatch(&frames[0]).unwrap_err();
    assert_eq!(catalog_mismatch.kind(), AndromedaErrorKind::Protocol);
    assert!(
        catalog_mismatch
            .message()
            .contains("expected route context"),
        "result-stream dispatch must reject CatalogVersion route drift"
    );

    let mut accepted = DispatchPolicy::new_result_stream(typed_result_stream_context());
    for frame in &frames {
        accepted.dispatch(frame).unwrap();
    }
    accepted.finish().unwrap();
}

#[test]
fn typed_result_stream_sequence_binds_to_admitted_route_context() {
    let frames = typed_result_stream_frames();
    validate_typed_result_stream_sequence_with_context_and_bounds(
        &frames,
        ResultStreamMetadataPolicy::RowBatchRequired,
        typed_result_stream_context(),
        TypedResultStreamBounds::v0_default(),
    )
    .unwrap();

    let wrong_session_context = TypedResultStreamContext::new(
        RequestId::new(501),
        SessionId::new(999),
        Some(TransactionId::new(701)),
        ContractHash::from_slice(&hash(7)).unwrap(),
        CatalogVersion::new(42),
    );
    let err = validate_typed_result_stream_sequence_with_context_and_bounds(
        &frames,
        ResultStreamMetadataPolicy::RowBatchRequired,
        wrong_session_context,
        TypedResultStreamBounds::v0_default(),
    )
    .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(
        err.message().contains("expected route context"),
        "typed result streams must reject session/contract drift against the admitted route"
    );
}

#[test]
fn typed_result_stream_sequence_enforces_bounded_policy_before_acceptance() {
    let frames = typed_result_stream_frames();

    let too_few_frames = TypedResultStreamBounds::new(2, 64 * 1024);
    assert_eq!(
        validate_typed_result_stream_sequence_with_context_and_bounds(
            &frames,
            ResultStreamMetadataPolicy::RowBatchRequired,
            typed_result_stream_context(),
            too_few_frames,
        )
        .unwrap_err()
        .kind(),
        AndromedaErrorKind::Resource
    );

    let byte_budget = (frames[0].payload.len() + frames[1].payload.len()) as u64;
    assert_eq!(
        validate_typed_result_stream_sequence_with_context_and_bounds(
            &frames,
            ResultStreamMetadataPolicy::RowBatchRequired,
            typed_result_stream_context(),
            TypedResultStreamBounds::new(3, byte_budget),
        )
        .unwrap_err()
        .kind(),
        AndromedaErrorKind::Resource
    );
}

#[test]
fn result_stream_projection_rejects_payload_kind_spoofing_before_payload_acceptance() {
    let batch_payload = encode_generated_message(&generated::protocol::v1::RpcBatch {
        result_name: "Inventory.ReserveStock.Reservation".to_string(),
        batch_index: 0,
        rows_emitted: 1,
        structured_payload: b"\x01".to_vec(),
        row_count_exact: Some(1),
        terminal_batch: true,
    });
    let completion_payload = encode_generated_message(&generated::protocol::v1::RpcCompletion {
        status: generated::protocol::v1::rpc_completion::Status::Committed as i32,
        rows_affected: Some(2),
        tx_id: Some(701),
        request_id: Some(501),
        session_id: Some(601),
        trace_id: Some("trace".to_string()),
        transaction_outcome: generated::protocol::v1::rpc_completion::TransactionOutcome::Committed
            as i32,
        durable_lsn: Some(3),
        result_row_counts: Vec::new(),
    });

    let frames = [
        frame(
            FrameType::RpcMetadata,
            envelope_payload(
                generated::protocol::v1::PayloadKind::RpcBatch,
                batch_payload.clone(),
            ),
        ),
        frame(
            FrameType::RpcBatch,
            envelope_payload(
                generated::protocol::v1::PayloadKind::RpcBatch,
                batch_payload,
            ),
        ),
        frame(
            FrameType::RpcCompletion,
            envelope_payload(
                generated::protocol::v1::PayloadKind::RpcCompletion,
                completion_payload,
            ),
        ),
    ];

    validate_result_stream_sequence(&frames)
        .expect("header-only order is insufficient without envelope projection");
    assert_eq!(
        decode_typed_frame_envelope(&frames[0]).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
    assert_eq!(
        validate_typed_result_stream_sequence(&frames)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );

    let mut proto_envelopes = Vec::new();
    for frame in &frames {
        let decoded_frame = FrameCodec::decode(&FrameCodec::encode(frame).unwrap()).unwrap();
        let decoded_envelope: generated::protocol::v1::FrameEnvelope =
            decode_generated_message(decoded_frame.payload.as_slice()).unwrap();
        proto_envelopes.push(proto_envelope_from_generated(decoded_envelope));
    }

    assert_eq!(proto_envelopes[0].payload_kind, PayloadKind::RpcBatch);
    assert_ne!(
        proto_envelopes[0].payload_kind.wire_code(),
        frames[0].header.frame_type.wire_code()
    );
    assert_eq!(
        ProtoFrameEnvelope::validate_rpc_stream_sequence(&proto_envelopes)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn typed_frame_envelope_rejects_frame_header_context_drift() {
    let mut metadata = frame(
        FrameType::RpcMetadata,
        envelope_payload(
            generated::protocol::v1::PayloadKind::RpcMetadata,
            encode_generated_message(&generated::protocol::v1::RpcMetadata {
                result_streams: Vec::new(),
                completion_policy: None,
            }),
        ),
    );
    metadata.header.request_id = RequestId::new(502);

    let err = decode_typed_frame_envelope(&metadata).unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(
        err.message().contains("context"),
        "typed envelope rejection should name frame/envelope context drift"
    );
}

#[test]
fn quic_proto_loopback_rejects_wrong_role_malformed_frame_and_bad_ordering() {
    let request = generated::protocol::v1::RpcExecuteRequest {
        procedure_name: "Inventory.ReserveStock".to_string(),
        expected_contract_hash: hash(7),
        expected_catalog_version: 42,
        surface_scope: "application".to_string(),
        arguments: Vec::new(),
        budget: None,
        expected_stats_version: Some(3),
    };
    let execute = frame(
        FrameType::RpcExecuteRequest,
        envelope_payload(
            generated::protocol::v1::PayloadKind::RpcExecuteRequest,
            encode_generated_message(&request),
        ),
    );

    assert_eq!(
        validate_single_frame_on_stream(&execute, StreamRole::ResultUnidirectional)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );

    let mut encoded = FrameCodec::encode(&execute).unwrap();
    encoded[FRAME_CODEC_CRC_OFFSET] ^= 0xFF;
    assert_eq!(
        FrameCodec::decode(&encoded).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );

    let metadata = frame(
        FrameType::RpcMetadata,
        envelope_payload(
            generated::protocol::v1::PayloadKind::RpcMetadata,
            encode_generated_message(&generated::protocol::v1::RpcMetadata {
                result_streams: Vec::new(),
                completion_policy: None,
            }),
        ),
    );
    let completion = frame(
        FrameType::RpcCompletion,
        envelope_payload(
            generated::protocol::v1::PayloadKind::RpcCompletion,
            encode_generated_message(&generated::protocol::v1::RpcCompletion {
                status: generated::protocol::v1::rpc_completion::Status::Committed as i32,
                rows_affected: Some(0),
                tx_id: Some(701),
                request_id: Some(501),
                session_id: Some(601),
                trace_id: None,
                transaction_outcome:
                    generated::protocol::v1::rpc_completion::TransactionOutcome::Committed as i32,
                durable_lsn: Some(3),
                result_row_counts: Vec::new(),
            }),
        ),
    );

    assert_eq!(
        validate_result_stream_sequence(&[completion.clone(), metadata.clone()])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );

    let decoded_completion_frame =
        FrameCodec::decode(&FrameCodec::encode(&completion).unwrap()).unwrap();
    let decoded_metadata_frame =
        FrameCodec::decode(&FrameCodec::encode(&metadata).unwrap()).unwrap();
    let proto_completion = proto_envelope_from_generated(
        decode_generated_message(decoded_completion_frame.payload.as_slice()).unwrap(),
    );
    let proto_metadata = proto_envelope_from_generated(
        decode_generated_message(decoded_metadata_frame.payload.as_slice()).unwrap(),
    );
    assert_eq!(
        ProtoFrameEnvelope::validate_rpc_stream_sequence(&[proto_completion, proto_metadata])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn backpressure_retry_metadata_is_bounded_and_structured() {
    let signal = BackpressureSignal {
        reason: BackpressureReason::ExecutionQueueSaturated,
        request_id: Some(RequestId::new(501)),
        retry_after_millis: Some(250),
    };
    signal.validate_retry_policy().unwrap();

    let missing_request_scope = BackpressureSignal {
        request_id: None,
        ..signal
    };
    assert_eq!(
        missing_request_scope
            .validate_retry_policy()
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Resource
    );

    let error = ErrorEnvelope {
        request_id: signal.request_id,
        session_id: Some(SessionId::new(601)),
        trace_id: Some("trace".to_string()),
        family: ErrorFamily::Resource,
        code: "BACKPRESSURE".to_string(),
        message: "execution queue saturated".to_string(),
        transaction_effect: TransactionEffect::NoTransaction,
        retry_disposition: RetryDisposition::Backpressure,
        retry_after_ms: None,
        backpressure: Some(BackpressureMetadata {
            retry_after_ms: signal.retry_after_millis,
            capacity_percent: Some(75),
            shed_load: true,
        }),
    };
    error.validate().unwrap();

    let malformed_error = ErrorEnvelope {
        backpressure: Some(BackpressureMetadata {
            capacity_percent: Some(101),
            ..error.backpressure.unwrap()
        }),
        ..error
    };
    assert_eq!(
        malformed_error.validate().unwrap_err().kind(),
        AndromedaErrorKind::Resource
    );
}
