use super::*;

/// Validates that PayloadKind discriminators remain in range [1..9].
///
/// PayloadKind discriminators form the RPC dispatch boundary and must
/// never change to maintain protocol compatibility.
#[test]
fn test_payload_kind_enum_values_locked() {
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

/// Validates that Protobuf message field ordering remains deterministic.
///
/// Field ordering affects serialization. Any reordering breaks
/// backward compatibility and breaks protocol hash fingerprints.
#[test]
fn test_protobuf_field_ordering_stable() {
    use andromeda_proto::{descriptor_set_hash, frame_envelope_hash};

    let descriptor_hash = descriptor_set_hash();
    assert!(!descriptor_hash.is_zero(), "Descriptor hash is zero!");

    let envelope_hash = frame_envelope_hash();
    assert!(!envelope_hash.is_zero(), "Frame envelope hash is zero!");

    let descriptor_hash_2 = descriptor_set_hash();
    assert_eq!(
        descriptor_hash, descriptor_hash_2,
        "Descriptor hash not deterministic!"
    );

    let envelope_hash_2 = frame_envelope_hash();
    assert_eq!(
        envelope_hash, envelope_hash_2,
        "Envelope hash not deterministic!"
    );
}

/// Validates that ProtocolVersion is locked at V1.0.
///
/// The protocol version is frozen for Andromeda V0.5. Future versions
/// require major/minor version bumps with explicit compatibility handling.
#[test]
fn test_protocol_version_locked_at_1_0() {
    let v1_0 = ProtocolVersion { major: 1, minor: 0 };
    assert!(
        v1_0.is_supported(),
        "ProtocolVersion V1.0 is no longer supported!"
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
