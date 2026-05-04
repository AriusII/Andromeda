use andromeda_core::{ContractHash, RequestId, SessionId, TransactionId};
use andromeda_proto::{
    decode_generated_message, encode_generated_message, generated, RPC_EXECUTE_REQUEST_WIRE_CODE,
};
use andromeda_quic::{
    validate_result_stream_sequence, FrameBytes, FrameCodec, FrameHeader, FrameType,
    FRAME_CODEC_HEADER_LEN, FRAME_HEADER_CRC_UNCHECKED,
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
    };
    let payload = envelope_payload(
        generated::protocol::v1::PayloadKind::RpcExecuteRequest,
        encode_generated_message(&request),
    );
    let encoded = FrameCodec::encode(&frame(FrameType::RpcExecuteRequest, payload)).unwrap();

    assert_eq!(&encoded[4..8], &RPC_EXECUTE_REQUEST_WIRE_CODE.to_be_bytes());
    assert!(encoded.len() > FRAME_CODEC_HEADER_LEN);

    let decoded_frame = FrameCodec::decode(&encoded).unwrap();
    let decoded_envelope: generated::protocol::v1::FrameEnvelope =
        decode_generated_message(decoded_frame.payload.as_slice()).unwrap();
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

    for frame in frames {
        let encoded = FrameCodec::encode(&frame).unwrap();
        assert_eq!(
            &encoded[4..8],
            &frame.header.frame_type.wire_code().to_be_bytes()
        );

        let decoded_frame = FrameCodec::decode(&encoded).unwrap();
        let decoded_envelope: generated::protocol::v1::FrameEnvelope =
            decode_generated_message(decoded_frame.payload.as_slice()).unwrap();

        assert_eq!(
            decoded_envelope.payload_kind as u32,
            decoded_frame.header.frame_type.wire_code()
        );
        assert!(!decoded_envelope.payload.is_empty());
    }
}
