use andromeda_core::{AndromedaErrorKind, CatalogVersion, ContractHash, RequestId, SessionId};
use andromeda_proto::{
    encode_generated_message,
    generated::contract::v1::catalog_procedure_manifest_resolution_request,
};
use andromeda_quic::{
    CatalogProcedureManifestResolutionRequest, CatalogProcedureManifestResolutionResponse,
    FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameCodec, FrameHeader, FrameType,
    ResultStreamMetadataPolicy, StreamRole, TransportSurface,
    catalog_manifest_resolution_request_frame, decode_catalog_manifest_resolution_response_frame,
    validate_result_stream_sequence, validate_result_stream_sequence_with_metadata_policy,
    validate_transport_surface,
};

fn request() -> CatalogProcedureManifestResolutionRequest {
    CatalogProcedureManifestResolutionRequest {
        protocol_major: 1,
        protocol_minor: 0,
        request_id: 501,
        trace_id: Some("trace-catalog-negative".to_string()),
        selector: Some(
            catalog_procedure_manifest_resolution_request::Selector::ProcedureName(
                "Inventory.ReserveStock".to_string(),
            ),
        ),
        expected_contract_hash: None,
        expected_catalog_version: Some(9),
        require_source_generator_ready: true,
    }
}

fn frame(frame_type: FrameType, payload: impl Into<Vec<u8>>) -> FrameBytes {
    let payload = payload.into();
    FrameBytes {
        header: FrameHeader {
            frame_type,
            request_id: RequestId::new(501),
            session_id: SessionId::new(601),
            tx_id: None,
            payload_length: payload.len() as u64,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload,
    }
}

fn response_frame_with_status(status: i32) -> FrameBytes {
    let response = CatalogProcedureManifestResolutionResponse {
        protocol_major: 1,
        protocol_minor: 0,
        request_id: 501,
        trace_id: Some("trace-catalog-negative".to_string()),
        status,
        manifest: None,
        resolved_contract_hash: None,
        resolved_catalog_version: None,
        current_catalog_version: Some(9),
        diagnostic_code: Some(format!("status-{status}")),
    };
    let response_payload = encode_generated_message(&response);
    let envelope = andromeda_proto::generated::protocol::v1::FrameEnvelope {
        protocol_version: Some(andromeda_proto::generated::protocol::v1::ProtocolVersion {
            major: 1,
            minor: 0,
        }),
        contract_hash: vec![0; ContractHash::LEN],
        catalog_version: 9,
        request_id: 501,
        session_id: 601,
        tx_id: None,
        payload_kind: andromeda_proto::generated::protocol::v1::PayloadKind::ContractResponse
            as i32,
        payload: response_payload,
    };

    frame(
        FrameType::ContractResponse,
        encode_generated_message(&envelope),
    )
}

#[test]
fn result_batch_before_metadata_rejected() {
    let batch = frame(FrameType::RpcBatch, b"row".to_vec());

    assert_eq!(
        validate_result_stream_sequence(&[batch])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn completion_before_batch_rejected_unless_policy_allows_zero_rows() {
    let metadata = frame(FrameType::RpcMetadata, b"metadata".to_vec());
    let completion = frame(FrameType::RpcCompletion, Vec::new());

    assert_eq!(
        validate_result_stream_sequence(&[metadata.clone(), completion.clone()])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );

    validate_result_stream_sequence_with_metadata_policy(
        &[metadata, completion],
        ResultStreamMetadataPolicy::ZeroRowCompletionAllowed,
    )
    .unwrap();
}

#[test]
fn batch_after_completion_rejected() {
    let metadata = frame(FrameType::RpcMetadata, b"metadata".to_vec());
    let batch = frame(FrameType::RpcBatch, b"row".to_vec());
    let completion = frame(FrameType::RpcCompletion, Vec::new());
    let late_batch = frame(FrameType::RpcBatch, b"late-row".to_vec());

    assert_eq!(
        validate_result_stream_sequence(&[metadata, batch, completion, late_batch])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn datagram_carrying_contract_payload_rejected() {
    let request_frame = catalog_manifest_resolution_request_frame(
        &request(),
        SessionId::new(601),
        None,
        ContractHash::zero(),
        CatalogVersion::new(9),
    )
    .unwrap();
    let decoded = FrameCodec::decode(&FrameCodec::encode(&request_frame).unwrap()).unwrap();

    assert_eq!(
        validate_transport_surface(&decoded, TransportSurface::Datagram)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
    assert_eq!(
        validate_transport_surface(
            &decoded,
            TransportSurface::ReliableStream(StreamRole::CommandBidirectional),
        )
        .unwrap()
        .frame_type,
        FrameType::ContractRequest
    );
}

#[test]
fn protobuf_unspecified_and_unknown_catalog_status_rejected() {
    for status in [0, 12] {
        let raw_frame = response_frame_with_status(status);
        let decoded = FrameCodec::decode(&FrameCodec::encode(&raw_frame).unwrap()).unwrap();

        assert_eq!(
            decode_catalog_manifest_resolution_response_frame(&decoded)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
    }
}
