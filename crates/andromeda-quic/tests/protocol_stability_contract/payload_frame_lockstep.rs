use super::*;

/// Validates that PayloadKind and FrameType discriminators stay synchronized.
///
/// The QUIC frame layer and Protobuf payload layer must maintain
/// synchronized discriminator mappings for correct dispatch.
#[test]
fn test_payload_to_frame_type_mapping_consistency() {
    use andromeda_proto::PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP;

    for (payload_kind, code) in PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP {
        let frame_type = FrameType::try_from(*code)
            .unwrap_or_else(|_| panic!("Frame type code {} not mapped", code));

        assert_eq!(
            payload_kind.wire_code(),
            frame_type.wire_code(),
            "PayloadKind and FrameType codes diverged for code {}",
            code
        );
        assert_eq!(
            PayloadKind::try_from(*code).unwrap(),
            *payload_kind,
            "PayloadKind round-trip failed for code {}",
            code
        );
        assert_eq!(
            FrameType::try_from(*code).unwrap(),
            frame_type,
            "FrameType round-trip failed for code {}",
            code
        );
    }
}

/// Validates that frame family mappings remain stable.
///
/// Frame families control stream role validation and dispatch routing.
/// Changes break transport surface separation.
#[test]
fn test_frame_family_mapping_preserved() {
    use andromeda_quic::FrameFamily;

    assert_eq!(FrameType::Hello.frame_family(), FrameFamily::SessionControl);
    assert_eq!(FrameType::Auth.frame_family(), FrameFamily::SessionControl);
    assert_eq!(
        FrameType::ContractRequest.frame_family(),
        FrameFamily::ContractControl
    );
    assert_eq!(
        FrameType::ContractResponse.frame_family(),
        FrameFamily::ContractControl
    );
    assert_eq!(
        FrameType::RpcExecuteRequest.frame_family(),
        FrameFamily::RpcCommand
    );
    assert_eq!(
        FrameType::RpcMetadata.frame_family(),
        FrameFamily::RpcResultStream
    );
    assert_eq!(
        FrameType::RpcBatch.frame_family(),
        FrameFamily::RpcResultStream
    );
    assert_eq!(
        FrameType::RpcCompletion.frame_family(),
        FrameFamily::RpcResultStream
    );
    assert_eq!(FrameType::Error.frame_family(), FrameFamily::Diagnostic);
    assert_eq!(
        FrameType::TelemetrySoftSignal.frame_family(),
        FrameFamily::Telemetry
    );
}

/// Validates that payload requirement rules remain stable.
///
/// Some frame types require non-empty payloads or contract hashes.
/// Changes break protocol validation.
#[test]
fn test_payload_requirements_preserved() {
    assert!(PayloadKind::RpcExecuteRequest.requires_non_empty_payload());
    assert!(FrameType::RpcExecuteRequest.requires_non_empty_payload());
    assert!(PayloadKind::RpcBatch.requires_non_empty_payload());
    assert!(FrameType::RpcBatch.requires_non_empty_payload());
    assert!(PayloadKind::Error.requires_non_empty_payload());
    assert!(FrameType::Error.requires_non_empty_payload());

    assert!(!PayloadKind::Hello.requires_non_empty_payload());
    assert!(!FrameType::Hello.requires_non_empty_payload());

    assert!(PayloadKind::RpcExecuteRequest.requires_contract_hash());
    assert!(PayloadKind::RpcMetadata.requires_contract_hash());
    assert!(PayloadKind::RpcBatch.requires_contract_hash());
    assert!(PayloadKind::RpcCompletion.requires_contract_hash());

    assert!(!PayloadKind::Hello.requires_contract_hash());
    assert!(!PayloadKind::Auth.requires_contract_hash());
    assert!(!PayloadKind::Error.requires_contract_hash());
}
