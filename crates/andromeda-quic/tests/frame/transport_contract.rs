use andromeda_core::{AndromedaErrorKind, RequestId, SessionId, TransactionId};
use andromeda_quic::{
    AUTH_FRAME_CODE, CONTRACT_REQUEST_FRAME_CODE, CONTRACT_RESPONSE_FRAME_CODE, ERROR_FRAME_CODE,
    FRAME_HEADER_CRC_UNCHECKED, FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP, FrameBytes, FrameFamily,
    FrameHeader, FrameType, HELLO_FRAME_CODE, MAX_FRAME_PAYLOAD_LENGTH, RPC_BATCH_FRAME_CODE,
    RPC_COMPLETION_FRAME_CODE, RPC_EXECUTE_REQUEST_FRAME_CODE, RPC_METADATA_FRAME_CODE, StreamRole,
    TELEMETRY_SOFT_SIGNAL_FRAME_CODE, validate_frame_sequence, validate_result_stream_sequence,
    validate_single_frame_on_stream,
};

fn header(frame_type: FrameType, payload_length: u64) -> FrameHeader {
    FrameHeader {
        frame_type,
        request_id: RequestId::new(100),
        session_id: SessionId::new(200),
        tx_id: Some(TransactionId::new(300)),
        payload_length,
        flags: 0,
        header_crc: FRAME_HEADER_CRC_UNCHECKED,
    }
}

fn frame(frame_type: FrameType, payload: impl Into<Vec<u8>>) -> FrameBytes {
    let payload = payload.into();
    FrameBytes {
        header: header(frame_type, payload.len() as u64),
        payload,
    }
}

#[test]
fn frame_type_wire_codes_lockstep_with_payload_contract_codes() {
    let expected = [
        (
            FrameType::Hello,
            HELLO_FRAME_CODE,
            FrameFamily::SessionControl,
        ),
        (
            FrameType::Auth,
            AUTH_FRAME_CODE,
            FrameFamily::SessionControl,
        ),
        (
            FrameType::ContractRequest,
            CONTRACT_REQUEST_FRAME_CODE,
            FrameFamily::ContractControl,
        ),
        (
            FrameType::ContractResponse,
            CONTRACT_RESPONSE_FRAME_CODE,
            FrameFamily::ContractControl,
        ),
        (
            FrameType::RpcExecuteRequest,
            RPC_EXECUTE_REQUEST_FRAME_CODE,
            FrameFamily::RpcCommand,
        ),
        (
            FrameType::RpcMetadata,
            RPC_METADATA_FRAME_CODE,
            FrameFamily::RpcResultStream,
        ),
        (
            FrameType::RpcBatch,
            RPC_BATCH_FRAME_CODE,
            FrameFamily::RpcResultStream,
        ),
        (
            FrameType::RpcCompletion,
            RPC_COMPLETION_FRAME_CODE,
            FrameFamily::RpcResultStream,
        ),
        (FrameType::Error, ERROR_FRAME_CODE, FrameFamily::Diagnostic),
    ];

    assert_eq!(FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP.len(), expected.len());

    for (frame_type, code, family) in expected {
        assert_eq!(frame_type.wire_code(), code);
        assert_eq!(FrameType::try_from(code).unwrap(), frame_type);
        assert_eq!(frame_type.frame_family(), family);
        assert!(
            FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP
                .iter()
                .any(
                    |(locked_type, locked_code)| *locked_type == frame_type && *locked_code == code
                )
        );
    }

    assert_eq!(
        FrameType::TelemetrySoftSignal.wire_code(),
        TELEMETRY_SOFT_SIGNAL_FRAME_CODE
    );
    assert_eq!(
        FrameType::TelemetrySoftSignal.frame_family(),
        FrameFamily::Telemetry
    );
    assert!(
        FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP
            .iter()
            .all(|(_, code)| *code != TELEMETRY_SOFT_SIGNAL_FRAME_CODE)
    );
    assert_eq!(
        FrameType::try_from(0).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
    assert_eq!(
        FrameType::try_from(10).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn stream_roles_enforce_frame_family_boundaries() {
    let cases = [
        (
            FrameType::Hello,
            StreamRole::SessionControl,
            b"hello".to_vec(),
        ),
        (
            FrameType::ContractRequest,
            StreamRole::CommandBidirectional,
            b"contract".to_vec(),
        ),
        (
            FrameType::RpcExecuteRequest,
            StreamRole::CommandBidirectional,
            b"execute".to_vec(),
        ),
        (
            FrameType::RpcMetadata,
            StreamRole::ResultUnidirectional,
            b"meta".to_vec(),
        ),
        (FrameType::Error, StreamRole::Diagnostic, b"error".to_vec()),
        (
            FrameType::TelemetrySoftSignal,
            StreamRole::TelemetryDatagram,
            b"soft".to_vec(),
        ),
    ];

    for (frame_type, role, payload) in cases {
        let frame = frame(frame_type, payload);
        assert!(validate_single_frame_on_stream(&frame, role).is_ok());

        let wrong_role = match role {
            StreamRole::SessionControl => StreamRole::CommandBidirectional,
            _ => StreamRole::SessionControl,
        };
        assert_eq!(
            validate_single_frame_on_stream(&frame, wrong_role)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
    }
}

#[test]
fn quic_datagram_is_telemetry_only_and_never_contract_bound_payload() {
    assert!(FrameType::TelemetrySoftSignal.allows_datagram());
    assert!(!FrameType::TelemetrySoftSignal.requires_reliable_stream());

    for frame_type in [
        FrameType::Hello,
        FrameType::Auth,
        FrameType::ContractRequest,
        FrameType::ContractResponse,
        FrameType::RpcExecuteRequest,
        FrameType::RpcMetadata,
        FrameType::RpcBatch,
        FrameType::RpcCompletion,
        FrameType::Error,
    ] {
        assert!(
            !frame_type.allows_datagram(),
            "{frame_type:?} must stay off DATAGRAM"
        );
        assert!(frame_type.requires_reliable_stream());

        let payload = if frame_type.requires_non_empty_payload() {
            b"body".to_vec()
        } else {
            Vec::new()
        };
        assert_eq!(
            validate_single_frame_on_stream(
                &frame(frame_type, payload),
                StreamRole::TelemetryDatagram
            )
            .unwrap_err()
            .kind(),
            AndromedaErrorKind::Protocol
        );
    }

    assert!(
        validate_single_frame_on_stream(
            &frame(FrameType::TelemetrySoftSignal, b"telemetry".to_vec()),
            StreamRole::TelemetryDatagram,
        )
        .is_ok()
    );
}

#[test]
fn frame_header_rejects_reserved_flags_oversize_payloads_and_length_mismatch() {
    let mut with_reserved_flags = header(FrameType::RpcExecuteRequest, 4);
    with_reserved_flags.flags = 0x1;
    assert_eq!(
        with_reserved_flags
            .validate_reserved_flags()
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );

    let oversized = header(FrameType::RpcExecuteRequest, MAX_FRAME_PAYLOAD_LENGTH + 1);
    assert_eq!(
        oversized.validate_max_payload_length().unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );

    let mismatched = FrameBytes {
        header: header(FrameType::RpcExecuteRequest, 8),
        payload: b"short".to_vec(),
    };
    assert_eq!(
        mismatched
            .validate(StreamRole::CommandBidirectional)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );

    let valid = frame(FrameType::RpcExecuteRequest, b"body".to_vec());
    assert!(valid.validate(StreamRole::CommandBidirectional).is_ok());
}

#[test]
fn header_crc_helper_accepts_locked_value_and_rejects_mismatch() {
    let mut header = header(FrameType::RpcExecuteRequest, 4);
    header.header_crc = 0xAABB_CCDD;

    assert!(header.validate_header_crc(0xAABB_CCDD).is_ok());
    assert_eq!(
        header.validate_header_crc(0xDEAD_BEEF).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn result_stream_sequence_requires_metadata_then_batch_then_completion() {
    let metadata = frame(FrameType::RpcMetadata, b"meta".to_vec());
    let batch = frame(FrameType::RpcBatch, b"row".to_vec());
    let completion = frame(FrameType::RpcCompletion, Vec::new());

    assert!(
        validate_result_stream_sequence(&[metadata.clone(), batch.clone(), completion.clone(),])
            .is_ok()
    );
    assert!(
        validate_frame_sequence(
            &[metadata.clone(), batch.clone(), completion.clone()],
            StreamRole::ResultUnidirectional,
        )
        .is_ok()
    );

    assert_eq!(
        validate_result_stream_sequence(&[metadata.clone(), completion.clone()])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
    assert_eq!(
        validate_result_stream_sequence(&[batch.clone(), metadata.clone()])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );

    let mut wrong_context = batch.clone();
    wrong_context.header.request_id = RequestId::new(999);
    assert_eq!(
        validate_result_stream_sequence(&[metadata.clone(), wrong_context])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );

    let wrong_type = frame(FrameType::Error, b"error".to_vec());
    assert_eq!(
        validate_result_stream_sequence(&[metadata, wrong_type])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
}
