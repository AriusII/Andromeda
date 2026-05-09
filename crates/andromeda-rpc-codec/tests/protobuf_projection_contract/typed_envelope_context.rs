use super::*;

fn route_context(
    request_id: RequestId,
    session_id: SessionId,
    tx_id: Option<TransactionId>,
    contract_hash_byte: u8,
    catalog_version: CatalogVersion,
) -> TypedResultStreamContext {
    TypedResultStreamContext::new(
        request_id,
        session_id,
        tx_id,
        ContractHash::from_slice(&hash(contract_hash_byte)).unwrap(),
        catalog_version,
    )
}

fn metadata_frame_with_envelope_context(
    request_id: u64,
    session_id: u64,
    tx_id: Option<u64>,
    contract_hash_byte: u8,
    catalog_version: u64,
) -> FrameBytes {
    frame(
        FrameType::RpcMetadata,
        encode_generated_message(&generated::protocol::v1::FrameEnvelope {
            protocol_version: Some(generated::protocol::v1::ProtocolVersion { major: 1, minor: 0 }),
            contract_hash: hash(contract_hash_byte),
            catalog_version,
            request_id,
            session_id,
            tx_id,
            payload_kind: generated::protocol::v1::PayloadKind::RpcMetadata as i32,
            payload: rpc_metadata_payload(Vec::new()),
        }),
    )
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
fn typed_result_stream_sequence_rejects_each_route_context_drift_field() {
    let frames = typed_result_stream_frames();
    let cases = [
        (
            "ContractHash",
            route_context(
                RequestId::new(501),
                SessionId::new(601),
                Some(TransactionId::new(701)),
                8,
                CatalogVersion::new(42),
            ),
        ),
        (
            "CatalogVersion",
            route_context(
                RequestId::new(501),
                SessionId::new(601),
                Some(TransactionId::new(701)),
                7,
                CatalogVersion::new(43),
            ),
        ),
        (
            "RequestId",
            route_context(
                RequestId::new(502),
                SessionId::new(601),
                Some(TransactionId::new(701)),
                7,
                CatalogVersion::new(42),
            ),
        ),
        (
            "SessionId",
            route_context(
                RequestId::new(501),
                SessionId::new(602),
                Some(TransactionId::new(701)),
                7,
                CatalogVersion::new(42),
            ),
        ),
        (
            "tx_id",
            route_context(
                RequestId::new(501),
                SessionId::new(601),
                Some(TransactionId::new(702)),
                7,
                CatalogVersion::new(42),
            ),
        ),
    ];

    for (field, context) in cases {
        let err = validate_typed_result_stream_sequence_with_context_and_bounds(
            &frames,
            ResultStreamMetadataPolicy::RowBatchRequired,
            context,
            TypedResultStreamBounds::v0_default(),
        )
        .unwrap_err();

        assert_eq!(err.kind(), AndromedaErrorKind::Protocol, "{field}");
        assert!(
            err.message().contains("expected route context"),
            "typed ResultStream must reject {field} drift against the admitted route"
        );
    }
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
    let mut request_drift = typed_result_stream_frames()[0].clone();
    request_drift.header.request_id = RequestId::new(502);
    let mut session_drift = typed_result_stream_frames()[0].clone();
    session_drift.header.session_id = SessionId::new(602);
    let mut tx_drift = typed_result_stream_frames()[0].clone();
    tx_drift.header.tx_id = Some(TransactionId::new(702));

    for (field, metadata) in [
        ("RequestId", request_drift),
        ("SessionId", session_drift),
        ("tx_id", tx_drift),
    ] {
        let err = decode_typed_frame_envelope(&metadata).unwrap_err();

        assert_eq!(err.kind(), AndromedaErrorKind::Protocol, "{field}");
        assert!(
            err.message().contains("context"),
            "typed envelope rejection should name {field} frame/envelope context drift"
        );
    }
}

#[test]
fn typed_frame_envelope_rejects_envelope_context_drift() {
    for (field, metadata) in [
        (
            "RequestId",
            metadata_frame_with_envelope_context(502, 601, Some(701), 7, 42),
        ),
        (
            "SessionId",
            metadata_frame_with_envelope_context(501, 602, Some(701), 7, 42),
        ),
        (
            "tx_id",
            metadata_frame_with_envelope_context(501, 601, Some(702), 7, 42),
        ),
    ] {
        let err = decode_typed_frame_envelope(&metadata).unwrap_err();

        assert_eq!(err.kind(), AndromedaErrorKind::Protocol, "{field}");
        assert!(
            err.message().contains("context"),
            "typed envelope rejection should name {field} envelope/header context drift"
        );
    }
}

#[test]
fn typed_result_stream_sequence_rejects_envelope_contract_and_catalog_route_drift() {
    for (field, metadata) in [
        (
            "ContractHash",
            metadata_frame_with_envelope_context(501, 601, Some(701), 8, 42),
        ),
        (
            "CatalogVersion",
            metadata_frame_with_envelope_context(501, 601, Some(701), 7, 43),
        ),
    ] {
        let mut frames = typed_result_stream_frames();
        frames[0] = metadata;
        let err = validate_typed_result_stream_sequence_with_context_and_bounds(
            &frames,
            ResultStreamMetadataPolicy::RowBatchRequired,
            typed_result_stream_context(),
            TypedResultStreamBounds::v0_default(),
        )
        .unwrap_err();

        assert_eq!(err.kind(), AndromedaErrorKind::Protocol, "{field}");
        assert!(
            err.message().contains("expected route context"),
            "typed ResultStream must reject {field} drift inside the envelope"
        );
    }
}

#[test]
fn rpc_proto_loopback_rejects_wrong_role_malformed_frame_and_bad_ordering() {
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
