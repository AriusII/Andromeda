use super::*;

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

    let wrong_contract_context =
        typed_result_stream_context_with(SessionId::new(601), 8, CatalogVersion::new(42));
    let mut wrong_context = DispatchPolicy::new_result_stream(wrong_contract_context);
    let mismatch = wrong_context.dispatch(&frames[0]).unwrap_err();
    assert_eq!(mismatch.kind(), AndromedaErrorKind::Protocol);
    assert!(
        mismatch.message().contains("expected route context"),
        "result-stream dispatch must reject ContractHash/CatalogVersion route drift"
    );

    let wrong_catalog_context =
        typed_result_stream_context_with(SessionId::new(601), 7, CatalogVersion::new(43));
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

    let wrong_session_context =
        typed_result_stream_context_with(SessionId::new(999), 7, CatalogVersion::new(42));
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

    let (_, decoded_completion) = roundtrip_generated_envelope(&completion);
    let (_, decoded_metadata) = roundtrip_generated_envelope(&metadata);
    let proto_completion = proto_envelope_from_generated(decoded_completion);
    let proto_metadata = proto_envelope_from_generated(decoded_metadata);
    assert_eq!(
        ProtoFrameEnvelope::validate_rpc_stream_sequence(&[proto_completion, proto_metadata])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
}
