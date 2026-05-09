use andromeda_error::AndromedaErrorKind;
use andromeda_proto::{
    FrameEnvelope as ProtoFrameEnvelope, PayloadKind, ProtocolVersion,
    RPC_EXECUTE_REQUEST_WIRE_CODE, decode_generated_message, encode_generated_message, generated,
};
use andromeda_rpc_codec::{
    TypedResultStreamBounds, TypedResultStreamContext, decode_typed_frame_envelope,
    validate_typed_result_stream_sequence,
    validate_typed_result_stream_sequence_with_context_and_bounds,
};
use andromeda_rpc_protocol::{
    FRAME_CODEC_CRC_OFFSET, FRAME_CODEC_HEADER_LEN, FRAME_HEADER_CRC_UNCHECKED, FrameBytes,
    FrameCodec, FrameHeader, FrameType, ResultStreamMetadataPolicy, StreamRole,
    validate_result_stream_sequence, validate_single_frame_on_stream,
};
use andromeda_types::{CatalogVersion, ContractHash, RequestId, SessionId, TransactionId};

#[path = "protobuf_projection_contract/execute_projection.rs"]
mod execute_projection;
#[path = "protobuf_projection_contract/procedure_gateway_decode.rs"]
mod procedure_gateway_decode;
#[path = "protobuf_projection_contract/result_stream_projection.rs"]
mod result_stream_projection;
#[path = "protobuf_projection_contract/typed_envelope_context.rs"]
mod typed_envelope_context;

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
    typed_result_stream_context_with(SessionId::new(601), 7, CatalogVersion::new(42))
}

fn typed_result_stream_context_with(
    session_id: SessionId,
    contract_hash_byte: u8,
    catalog_version: CatalogVersion,
) -> TypedResultStreamContext {
    TypedResultStreamContext::new(
        RequestId::new(501),
        session_id,
        Some(TransactionId::new(701)),
        ContractHash::from_slice(&hash(contract_hash_byte)).unwrap(),
        catalog_version,
    )
}

fn typed_result_stream_frames() -> [FrameBytes; 3] {
    result_stream_frames_with_payloads(
        rpc_metadata_payload(Vec::new()),
        rpc_batch_payload(),
        rpc_completion_payload(Vec::new()),
    )
}

fn result_stream_frames_with_payloads(
    metadata_payload: Vec<u8>,
    batch_payload: Vec<u8>,
    completion_payload: Vec<u8>,
) -> [FrameBytes; 3] {
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

fn rpc_metadata_payload(
    result_streams: Vec<generated::contract::v1::ResultStreamDescriptor>,
) -> Vec<u8> {
    encode_generated_message(&generated::protocol::v1::RpcMetadata {
        result_streams,
        completion_policy: Some(generated::protocol::v1::ResultCompletionPolicy {
            completion_shape:
                generated::protocol::v1::result_completion_policy::CompletionShape::RequiresRowBatch
                    as i32,
            reason: "requires batch".to_string(),
        }),
    })
}

fn rpc_batch_payload() -> Vec<u8> {
    encode_generated_message(&generated::protocol::v1::RpcBatch {
        result_name: "Inventory.ReserveStock.Reservation".to_string(),
        batch_index: 0,
        rows_emitted: 1,
        structured_payload: b"\x01".to_vec(),
        row_count_exact: Some(1),
        terminal_batch: true,
    })
}

fn rpc_completion_payload(
    result_row_counts: Vec<generated::protocol::v1::rpc_completion::ResultRowCountSummary>,
) -> Vec<u8> {
    encode_generated_message(&generated::protocol::v1::RpcCompletion {
        status: generated::protocol::v1::rpc_completion::Status::Committed as i32,
        rows_affected: Some(2),
        tx_id: Some(701),
        request_id: Some(501),
        session_id: Some(601),
        trace_id: Some("trace".to_string()),
        transaction_outcome: generated::protocol::v1::rpc_completion::TransactionOutcome::Committed
            as i32,
        durable_lsn: Some(3),
        result_row_counts,
    })
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

fn roundtrip_generated_envelope(
    frame: &FrameBytes,
) -> (FrameBytes, generated::protocol::v1::FrameEnvelope) {
    let encoded = FrameCodec::encode(frame).unwrap();
    assert_eq!(
        &encoded[4..8],
        &frame.header.frame_type.wire_code().to_be_bytes()
    );

    let decoded_frame = FrameCodec::decode(&encoded).unwrap();
    let decoded_envelope: generated::protocol::v1::FrameEnvelope =
        decode_generated_message(decoded_frame.payload.as_slice()).unwrap();
    (decoded_frame, decoded_envelope)
}
