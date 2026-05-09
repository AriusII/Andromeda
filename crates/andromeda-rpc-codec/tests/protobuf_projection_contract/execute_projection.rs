use super::*;

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
