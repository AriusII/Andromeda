use andromeda_proto_wire::{
    PayloadKind, ProtocolVersion, descriptor_set_hash, frame_envelope_hash,
};
use andromeda_rpc_protocol::{PayloadKindInvariants, ProtocolVersionInvariants};

#[test]
fn payload_kind_enum_values_are_locked() {
    let locked_codes = [
        (PayloadKind::Hello, 1, "PayloadKind::Hello"),
        (PayloadKind::Auth, 2, "PayloadKind::Auth"),
        (
            PayloadKind::ContractRequest,
            3,
            "PayloadKind::ContractRequest",
        ),
        (
            PayloadKind::ContractResponse,
            4,
            "PayloadKind::ContractResponse",
        ),
        (
            PayloadKind::RpcExecuteRequest,
            5,
            "PayloadKind::RpcExecuteRequest",
        ),
        (PayloadKind::RpcMetadata, 6, "PayloadKind::RpcMetadata"),
        (PayloadKind::RpcBatch, 7, "PayloadKind::RpcBatch"),
        (PayloadKind::RpcCompletion, 8, "PayloadKind::RpcCompletion"),
        (PayloadKind::Error, 9, "PayloadKind::Error"),
    ];

    for (kind, expected_code, label) in locked_codes {
        assert_eq!(kind.wire_code(), expected_code, "{label} code changed");
    }

    assert!(
        PayloadKindInvariants::validate().is_ok(),
        "PayloadKind invariants validation failed"
    );

    for code in 1..=9 {
        assert!(
            PayloadKind::try_from(code).is_ok(),
            "Code {} should be valid",
            code
        );
    }

    for code in [0, 10, 100] {
        assert!(
            PayloadKind::try_from(code).is_err(),
            "Code {} should be invalid",
            code
        );
    }
}

#[test]
fn schema_hash_helpers_are_deterministic() {
    let descriptor_bytes = b"andromeda.protocol.v1.FrameEnvelope";
    let descriptor_hash = descriptor_set_hash(descriptor_bytes);
    assert!(!descriptor_hash.is_zero(), "Descriptor hash is zero");

    let envelope_hash = frame_envelope_hash(descriptor_bytes);
    assert!(!envelope_hash.is_zero(), "Frame envelope hash is zero");

    assert_eq!(
        descriptor_hash,
        descriptor_set_hash(descriptor_bytes),
        "Descriptor hash not deterministic"
    );
    assert_eq!(
        envelope_hash,
        frame_envelope_hash(descriptor_bytes),
        "Envelope hash not deterministic"
    );
}

#[test]
fn protocol_version_is_locked_at_1_0() {
    let v1_0 = ProtocolVersion { major: 1, minor: 0 };
    assert!(
        v1_0.is_supported(),
        "ProtocolVersion V1.0 is no longer supported"
    );
    assert!(v1_0.validate().is_ok(), "V1.0 validation failed");

    assert_eq!(
        ProtocolVersion::SUPPORTED_MAJOR,
        1,
        "Supported major version changed"
    );
    assert_eq!(
        ProtocolVersion::SUPPORTED_MINOR,
        0,
        "Supported minor version changed"
    );

    assert!(
        v1_0.is_compatible_with(v1_0),
        "V1.0 is not compatible with itself"
    );

    assert!(
        ProtocolVersionInvariants::validate().is_ok(),
        "Protocol version invariants validation failed"
    );

    let v1_1 = ProtocolVersion { major: 1, minor: 1 };
    assert!(
        v1_1.validate().is_err(),
        "V1.1 should be rejected in locked protocol"
    );

    let v2_0 = ProtocolVersion { major: 2, minor: 0 };
    assert!(
        v2_0.validate().is_err(),
        "V2.0 should be rejected in locked protocol"
    );
}
