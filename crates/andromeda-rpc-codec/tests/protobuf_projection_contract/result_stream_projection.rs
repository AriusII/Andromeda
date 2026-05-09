use super::*;

#[test]
fn result_stream_frames_carry_generated_metadata_batch_completion_payloads() {
    let frames = result_stream_frames_with_payloads(
        rpc_metadata_payload(vec![generated::contract::v1::ResultStreamDescriptor {
            stream_name: "Reservation".to_string(),
            columns: vec![generated::contract::v1::ColumnDescriptor {
                name: "Reserved".to_string(),
                ordinal: 1,
                type_name: "bool".to_string(),
            }],
            cardinality: generated::contract::v1::result_stream_descriptor::Cardinality::ExactlyOne
                as i32,
            row_count_requirement: generated::contract::v1::result_stream_descriptor::RowCountRequirement::ExactRequired
                as i32,
            row_count_exact: Some(1),
            row_count_max: None,
        }]),
        rpc_batch_payload(),
        rpc_completion_payload(vec![
            generated::protocol::v1::rpc_completion::ResultRowCountSummary {
                result_name: "Inventory.ReserveStock.Reservation".to_string(),
                rows_emitted: 1,
                row_count_exact: Some(1),
            },
        ]),
    );

    validate_result_stream_sequence(&frames).unwrap();
    let mut proto_envelopes = Vec::new();

    for frame in &frames {
        let (decoded_frame, decoded_envelope) = roundtrip_generated_envelope(frame);
        let proto_envelope = proto_envelope_from_generated(decoded_envelope.clone());
        proto_envelope.validate().unwrap();

        assert_eq!(
            decoded_envelope.payload_kind as u32,
            decoded_frame.header.frame_type.wire_code()
        );
        assert!(!decoded_envelope.payload.is_empty());
        proto_envelopes.push(proto_envelope);
    }

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
fn proto_result_stream_projection_rejects_envelope_context_drift_fields() {
    let frames = typed_result_stream_frames();
    let proto_envelopes = frames
        .iter()
        .map(|frame| {
            let (_, decoded_envelope) = roundtrip_generated_envelope(frame);
            proto_envelope_from_generated(decoded_envelope)
        })
        .collect::<Vec<_>>();

    for field in [
        "ContractHash",
        "CatalogVersion",
        "RequestId",
        "SessionId",
        "tx_id",
    ] {
        let mut drifted = proto_envelopes.clone();
        match field {
            "ContractHash" => {
                drifted[1].contract_hash = ContractHash::from_slice(&hash(8)).unwrap();
            },
            "CatalogVersion" => {
                drifted[1].catalog_version = CatalogVersion::new(43);
            },
            "RequestId" => {
                drifted[1].request_id = RequestId::new(502);
            },
            "SessionId" => {
                drifted[1].session_id = SessionId::new(602);
            },
            "tx_id" => {
                drifted[1].tx_id = Some(TransactionId::new(702));
            },
            _ => unreachable!(),
        }

        let err = ProtoFrameEnvelope::validate_rpc_stream_sequence(&drifted).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol, "{field}");
        assert!(
            err.message().contains("request context"),
            "ProtoFrameEnvelope sequence must reject {field} drift"
        );
    }
}
#[test]
fn result_stream_projection_rejects_payload_kind_spoofing_before_payload_acceptance() {
    let batch_payload = rpc_batch_payload();
    let completion_payload = rpc_completion_payload(Vec::new());

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
        let (_, decoded_envelope) = roundtrip_generated_envelope(frame);
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
