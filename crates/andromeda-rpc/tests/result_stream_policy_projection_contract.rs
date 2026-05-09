use andromeda_error::AndromedaErrorKind;
use andromeda_proto_wire::{
    GeneratedFrameEnvelope, GeneratedPayloadKind, GeneratedProtocolVersion, GeneratedRpcBatch,
    GeneratedRpcCompletion, GeneratedRpcCompletionStatus, GeneratedRpcMetadata,
    GeneratedTransactionOutcome, encode_protobuf_message,
};
use andromeda_rpc::DispatchPolicy;
use andromeda_rpc_codec::TypedResultStreamContext;
use andromeda_rpc_protocol::{
    FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameHeader, FrameType, StreamRole,
};
use andromeda_types::{CatalogVersion, ContractHash, RequestId, SessionId, TransactionId};

fn hash(byte: u8) -> Vec<u8> {
    vec![byte; ContractHash::LEN]
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

fn typed_result_stream_context() -> TypedResultStreamContext {
    typed_result_stream_context_with(SessionId::new(601), 7, CatalogVersion::new(42))
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

fn envelope_payload(kind: GeneratedPayloadKind, payload: Vec<u8>) -> Vec<u8> {
    encode_protobuf_message(&GeneratedFrameEnvelope {
        protocol_version: Some(GeneratedProtocolVersion { major: 1, minor: 0 }),
        contract_hash: hash(7),
        catalog_version: 42,
        request_id: 501,
        session_id: 601,
        tx_id: Some(701),
        payload_kind: kind as i32,
        payload,
    })
}

fn typed_result_stream_frames() -> [FrameBytes; 3] {
    [
        frame(
            FrameType::RpcMetadata,
            envelope_payload(
                GeneratedPayloadKind::RpcMetadata,
                encode_protobuf_message(&GeneratedRpcMetadata {
                    result_streams: Vec::new(),
                    completion_policy: None,
                }),
            ),
        ),
        frame(
            FrameType::RpcBatch,
            envelope_payload(
                GeneratedPayloadKind::RpcBatch,
                encode_protobuf_message(&GeneratedRpcBatch {
                    result_name: "Inventory.ReserveStock.Reservation".to_string(),
                    batch_index: 0,
                    rows_emitted: 1,
                    structured_payload: b"\x01".to_vec(),
                    row_count_exact: Some(1),
                    terminal_batch: true,
                }),
            ),
        ),
        frame(
            FrameType::RpcCompletion,
            envelope_payload(
                GeneratedPayloadKind::RpcCompletion,
                encode_protobuf_message(&GeneratedRpcCompletion {
                    status: GeneratedRpcCompletionStatus::Committed as i32,
                    rows_affected: Some(2),
                    tx_id: Some(701),
                    request_id: Some(501),
                    session_id: Some(601),
                    trace_id: Some("trace".to_string()),
                    transaction_outcome: GeneratedTransactionOutcome::Committed as i32,
                    durable_lsn: Some(3),
                    result_row_counts: Vec::new(),
                }),
            ),
        ),
    ]
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
