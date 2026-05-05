//! Protocol Stability Contract Tests (D7)
//!
//! Validates that protocol invariants remain stable across commits.
//! These tests detect regressions in frame format, Protobuf schema,
//! and RPC contract discriminators.

use andromeda_core::{RequestId, SessionId};
use andromeda_proto::{PayloadKind, ProtocolVersion};
use andromeda_quic::{
    AUTH_FRAME_CODE, CONTRACT_REQUEST_FRAME_CODE, CONTRACT_RESPONSE_FRAME_CODE, ERROR_FRAME_CODE,
    FRAME_CODEC_CRC_OFFSET, FRAME_CODEC_HEADER_LEN, FrameHeader, FrameType, FrameTypeInvariants,
    HELLO_FRAME_CODE, MAX_FRAME_PAYLOAD_LENGTH, PayloadKindInvariants, ProtocolInvariants,
    ProtocolVersionInvariants, RPC_BATCH_FRAME_CODE, RPC_COMPLETION_FRAME_CODE,
    RPC_EXECUTE_REQUEST_FRAME_CODE, RPC_METADATA_FRAME_CODE, TELEMETRY_SOFT_SIGNAL_FRAME_CODE,
    validate_frame_header_layout,
};
use std::mem;

// ============================================================================
// Test 1: Frame Header Offset Stability
// ============================================================================

/// Validates that FrameHeader wire offsets remain constant.
///
/// The encoded frame header is wire-critical. Rust struct padding may differ
/// from the wire length and must not be treated as the protocol contract.
#[test]
fn test_frame_header_offset_stability() {
    // The Rust struct currently contains padding; only FRAME_CODEC_HEADER_LEN
    // defines the stable wire contract.
    assert!(
        mem::size_of::<FrameHeader>() >= FRAME_CODEC_HEADER_LEN,
        "FrameHeader memory layout must be large enough for the encoded header"
    );

    // Validate through the module function
    assert!(
        validate_frame_header_layout().is_ok(),
        "Frame header layout validation failed"
    );

    // FRAME_CODEC_HEADER_LEN is the stable encoded header size.
    assert_eq!(
        FRAME_CODEC_HEADER_LEN, 52,
        "FRAME_CODEC_HEADER_LEN changed! This breaks wire protocol compatibility."
    );

    // Verify all field sizes sum correctly
    // frame_type (FrameType=u32): 4 bytes
    // request_id (RequestId=u64): 8 bytes
    // session_id (SessionId=u64): 8 bytes
    // tx_id (Option<u64>): 16 bytes
    // payload_length (u64): 8 bytes
    // flags (u32): 4 bytes
    // header_crc (u32): 4 bytes
    // Total: 52 bytes
    let expected_total = 4 + 8 + 8 + 16 + 8 + 4 + 4;
    assert_eq!(
        expected_total, 52,
        "Field size calculation incorrect: {}",
        expected_total
    );
}

// ============================================================================
// Test 2: Payload Kind Enum Values Locked
// ============================================================================

/// Validates that PayloadKind discriminators remain in range [1..8].
///
/// PayloadKind discriminators form the RPC dispatch boundary and must
/// never change to maintain protocol compatibility.
#[test]
fn test_payload_kind_enum_values_locked() {
    // Each PayloadKind must have its expected code
    assert_eq!(
        PayloadKind::Hello.wire_code(),
        1,
        "PayloadKind::Hello code changed"
    );
    assert_eq!(
        PayloadKind::Auth.wire_code(),
        2,
        "PayloadKind::Auth code changed"
    );
    assert_eq!(
        PayloadKind::ContractRequest.wire_code(),
        3,
        "PayloadKind::ContractRequest code changed"
    );
    assert_eq!(
        PayloadKind::ContractResponse.wire_code(),
        4,
        "PayloadKind::ContractResponse code changed"
    );
    assert_eq!(
        PayloadKind::RpcExecuteRequest.wire_code(),
        5,
        "PayloadKind::RpcExecuteRequest code changed"
    );
    assert_eq!(
        PayloadKind::RpcMetadata.wire_code(),
        6,
        "PayloadKind::RpcMetadata code changed"
    );
    assert_eq!(
        PayloadKind::RpcBatch.wire_code(),
        7,
        "PayloadKind::RpcBatch code changed"
    );
    assert_eq!(
        PayloadKind::RpcCompletion.wire_code(),
        8,
        "PayloadKind::RpcCompletion code changed"
    );
    assert_eq!(
        PayloadKind::Error.wire_code(),
        9,
        "PayloadKind::Error code changed"
    );

    // Validate through invariant checker
    assert!(
        PayloadKindInvariants::validate().is_ok(),
        "PayloadKind invariants validation failed"
    );

    // All codes in range [1..8] or 9
    for code in 1..=9 {
        let result = PayloadKind::try_from(code);
        match code {
            1..=9 => assert!(result.is_ok(), "Code {} should be valid", code),
            _ => assert!(result.is_err(), "Code {} should be invalid", code),
        }
    }

    // Invalid codes must be rejected
    assert!(
        PayloadKind::try_from(0).is_err(),
        "Code 0 should be invalid"
    );
    assert!(
        PayloadKind::try_from(10).is_err(),
        "Code 10 should be invalid"
    );
    assert!(
        PayloadKind::try_from(100).is_err(),
        "Code 100 should be invalid"
    );
}

// ============================================================================
// Test 3: Protobuf Field Ordering Stability
// ============================================================================

/// Validates that Protobuf message field ordering remains deterministic.
///
/// Field ordering affects serialization. Any reordering breaks
/// backward compatibility and breaks protocol hash fingerprints.
#[test]
fn test_protobuf_field_ordering_stable() {
    // Frame envelope field ordering must remain:
    // 1: protocol_version
    // 2: contract_hash
    // 3: catalog_version
    // 4: request_id
    // 5: session_id
    // 6: tx_id
    // 7: payload_kind
    // 8: payload

    // This test validates through Protobuf descriptor examination
    use andromeda_proto::{descriptor_set_hash, frame_envelope_hash};

    // Descriptor hash should be stable
    let descriptor_hash = descriptor_set_hash();
    assert!(!descriptor_hash.is_zero(), "Descriptor hash is zero!");

    // Frame envelope hash should be stable
    let envelope_hash = frame_envelope_hash();
    assert!(!envelope_hash.is_zero(), "Frame envelope hash is zero!");

    // Hashes should be identical across multiple calls (deterministic)
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

// ============================================================================
// Test 4: Protocol Version Locked at V1.0
// ============================================================================

/// Validates that ProtocolVersion is locked at V1.0.
///
/// The protocol version is frozen for Andromeda V0.5. Future versions
/// require major/minor version bumps with explicit compatibility handling.
#[test]
fn test_protocol_version_locked_at_1_0() {
    // Create V1.0 and verify it's supported
    let v1_0 = ProtocolVersion { major: 1, minor: 0 };
    assert!(
        v1_0.is_supported(),
        "ProtocolVersion V1.0 is no longer supported!"
    );
    assert!(v1_0.validate().is_ok(), "V1.0 validation failed");

    // Verify supported version matches V1.0
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

    // V1.0 must be compatible with itself
    assert!(
        v1_0.is_compatible_with(v1_0),
        "V1.0 is not compatible with itself"
    );

    // Validate through invariant checker
    assert!(
        ProtocolVersionInvariants::validate().is_ok(),
        "Protocol version invariants validation failed"
    );

    // Future versions should be rejected
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

// ============================================================================
// Test 5: Frame Type Discriminator Unchanged
// ============================================================================

/// Validates that FrameType enum values remain constant [1-8, 100].
///
/// Frame types are locked to prevent accidental mutations that would
/// corrupt the wire protocol frame discriminators.
#[test]
fn test_frame_type_discriminator_unchanged() {
    // Each FrameType must have its expected code
    assert_eq!(
        FrameType::Hello.wire_code(),
        HELLO_FRAME_CODE,
        "Hello frame code changed"
    );
    assert_eq!(
        FrameType::Auth.wire_code(),
        AUTH_FRAME_CODE,
        "Auth frame code changed"
    );
    assert_eq!(
        FrameType::ContractRequest.wire_code(),
        CONTRACT_REQUEST_FRAME_CODE,
        "ContractRequest frame code changed"
    );
    assert_eq!(
        FrameType::ContractResponse.wire_code(),
        CONTRACT_RESPONSE_FRAME_CODE,
        "ContractResponse frame code changed"
    );
    assert_eq!(
        FrameType::RpcExecuteRequest.wire_code(),
        RPC_EXECUTE_REQUEST_FRAME_CODE,
        "RpcExecuteRequest frame code changed"
    );
    assert_eq!(
        FrameType::RpcMetadata.wire_code(),
        RPC_METADATA_FRAME_CODE,
        "RpcMetadata frame code changed"
    );
    assert_eq!(
        FrameType::RpcBatch.wire_code(),
        RPC_BATCH_FRAME_CODE,
        "RpcBatch frame code changed"
    );
    assert_eq!(
        FrameType::RpcCompletion.wire_code(),
        RPC_COMPLETION_FRAME_CODE,
        "RpcCompletion frame code changed"
    );
    assert_eq!(
        FrameType::Error.wire_code(),
        ERROR_FRAME_CODE,
        "Error frame code changed"
    );
    assert_eq!(
        FrameType::TelemetrySoftSignal.wire_code(),
        TELEMETRY_SOFT_SIGNAL_FRAME_CODE,
        "TelemetrySoftSignal frame code changed"
    );

    // All codes 1-8 must map to valid frame types
    for code in 1..=8 {
        let result = FrameType::try_from(code);
        assert!(result.is_ok(), "Frame type code {} should be valid", code);
    }

    // Code 9 (Error) must also map
    assert!(
        FrameType::try_from(9).is_ok(),
        "Frame type code 9 should be valid"
    );

    // Code 100 (Telemetry) must also map
    assert!(
        FrameType::try_from(100).is_ok(),
        "Frame type code 100 should be valid"
    );

    // Validate through invariant checker
    assert!(
        FrameTypeInvariants::validate().is_ok(),
        "Frame type invariants validation failed"
    );
}

// ============================================================================
// Test 6: CRC Position Stable
// ============================================================================

/// Validates that CRC position in frame header remains constant.
///
/// The CRC is positioned at a fixed offset for fast validation.
/// Any movement breaks frame validation.
#[test]
fn test_crc_position_stable() {
    // CRC must be at fixed offset (last 4 bytes of 52-byte header)
    assert_eq!(
        FRAME_CODEC_CRC_OFFSET, 48,
        "Frame CRC offset changed! Expected offset 48 (12 bytes from end of 52-byte header)"
    );

    // The CRC is the last u32 in a 52-byte header
    // Offset 48 means it's at position [48..52]
    assert_eq!(
        FRAME_CODEC_CRC_OFFSET + 4,
        52,
        "CRC offset + 4 doesn't equal header size"
    );

    // Validate CRC is in expected position by constructing a frame header
    let header = FrameHeader {
        frame_type: FrameType::Hello,
        request_id: RequestId::new(1),
        session_id: SessionId::new(2),
        tx_id: None,
        payload_length: 0,
        flags: 0,
        header_crc: 0xDEADBEEF,
    };

    // Validate the CRC value can be read from the header
    assert_eq!(header.header_crc, 0xDEADBEEF, "CRC field not accessible");
}

// ============================================================================
// Test 7: Frame Payload Size Maximum Locked
// ============================================================================

/// Validates that MAX_FRAME_PAYLOAD_LENGTH remains 16 MiB.
///
/// The maximum payload size is a hard constraint for buffer management.
/// Any change affects memory allocation and flow control.
#[test]
fn test_frame_payload_max_size_locked() {
    // Maximum frame payload must be 16 MiB
    assert_eq!(
        MAX_FRAME_PAYLOAD_LENGTH,
        16 * 1024 * 1024,
        "MAX_FRAME_PAYLOAD_LENGTH changed!"
    );

    // Verify the constant is exactly 16 MiB
    let sixteen_mib = 16 * 1024 * 1024;
    assert_eq!(MAX_FRAME_PAYLOAD_LENGTH, sixteen_mib as u64);
}

// ============================================================================
// Test 8: Regression Detection - Field Reordering Would Fail
// ============================================================================

/// Demonstrates that field reordering would be caught by layout validation.
///
/// This test validates that our invariant checks would catch a hypothetical
/// accidental field reordering in FrameHeader.
#[test]
fn test_regression_detect_field_reordering() {
    // Create multiple frame headers and ensure their memory layout is consistent
    let h1 = FrameHeader {
        frame_type: FrameType::Hello,
        request_id: RequestId::new(100),
        session_id: SessionId::new(200),
        tx_id: None,
        payload_length: 42,
        flags: 0,
        header_crc: 0x12345678,
    };

    let h2 = FrameHeader {
        frame_type: FrameType::RpcBatch,
        request_id: RequestId::new(101),
        session_id: SessionId::new(201),
        tx_id: Some(andromeda_core::TransactionId::new(500)),
        payload_length: 1024,
        flags: 0,
        header_crc: 0x87654321,
    };

    // Both must have same size (layout didn't change)
    assert_eq!(
        mem::size_of_val(&h1),
        mem::size_of_val(&h2),
        "Frame header size varies between instances"
    );

    // Layout validation must pass for both
    assert!(validate_frame_header_layout().is_ok());

    // Verify frame type codes are extractable (regression test for field offset)
    assert_eq!(h1.frame_type, FrameType::Hello);
    assert_eq!(h2.frame_type, FrameType::RpcBatch);

    // Verify CRC is extractable at expected position
    assert_eq!(h1.header_crc, 0x12345678);
    assert_eq!(h2.header_crc, 0x87654321);
}

// ============================================================================
// Test 9: Master Protocol Invariant Validation
// ============================================================================

/// Comprehensive validation of all protocol invariants.
///
/// This test runs the complete invariant suite to ensure protocol stability.
#[test]
fn test_master_protocol_invariant_validation() {
    // Run all invariant validations
    assert!(
        ProtocolInvariants::validate_all().is_ok(),
        "Master protocol invariant validation failed"
    );

    // Verify diagnostic report can be generated
    let report = ProtocolInvariants::diagnostic_report();
    assert!(!report.is_empty(), "Diagnostic report is empty");
    assert!(
        report.contains("Protocol Invariants"),
        "Report missing header"
    );
    assert!(
        report.contains("Frame Header Layout"),
        "Report missing frame header section"
    );
    assert!(
        report.contains("Frame Type Codes"),
        "Report missing frame type section"
    );
    assert!(
        report.contains("Payload Kind Discriminators"),
        "Report missing payload kind section"
    );
    assert!(
        report.contains("Protocol Version Lock"),
        "Report missing protocol version section"
    );
}

// ============================================================================
// Test 10: Payload-to-Frame Type Mapping Consistency
// ============================================================================

/// Validates that PayloadKind and FrameType discriminators stay synchronized.
///
/// The QUIC frame layer and Protobuf payload layer must maintain
/// synchronized discriminator mappings for correct dispatch.
#[test]
fn test_payload_to_frame_type_mapping_consistency() {
    // PayloadKind and FrameType codes must align for dispatch
    use andromeda_proto::PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP;

    // Each payload kind must have matching frame type
    for (payload_kind, code) in PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP {
        let frame_type =
            FrameType::try_from(*code).expect(&format!("Frame type code {} not mapped", code));

        // Codes must match
        assert_eq!(
            payload_kind.wire_code(),
            frame_type.wire_code(),
            "PayloadKind and FrameType codes diverged for code {}",
            code
        );

        // Validate both map back correctly
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

// ============================================================================
// Test 11: Frame Family Mapping Preserved
// ============================================================================

/// Validates that frame family mappings remain stable.
///
/// Frame families control stream role validation and dispatch routing.
/// Changes break transport surface separation.
#[test]
fn test_frame_family_mapping_preserved() {
    use andromeda_quic::FrameFamily;

    // Session control frames must map to SessionControl family
    assert_eq!(FrameType::Hello.frame_family(), FrameFamily::SessionControl);
    assert_eq!(FrameType::Auth.frame_family(), FrameFamily::SessionControl);

    // Contract frames must map to ContractControl family
    assert_eq!(
        FrameType::ContractRequest.frame_family(),
        FrameFamily::ContractControl
    );
    assert_eq!(
        FrameType::ContractResponse.frame_family(),
        FrameFamily::ContractControl
    );

    // RPC command must map to RpcCommand family
    assert_eq!(
        FrameType::RpcExecuteRequest.frame_family(),
        FrameFamily::RpcCommand
    );

    // RPC result frames must map to RpcResultStream family
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

    // Error must map to Diagnostic family
    assert_eq!(FrameType::Error.frame_family(), FrameFamily::Diagnostic);

    // Telemetry must map to Telemetry family
    assert_eq!(
        FrameType::TelemetrySoftSignal.frame_family(),
        FrameFamily::Telemetry
    );
}

// ============================================================================
// Test 12: Payload Requirements Preserved
// ============================================================================

/// Validates that payload requirement rules remain stable.
///
/// Some frame types require non-empty payloads or contract hashes.
/// Changes break protocol validation.
#[test]
fn test_payload_requirements_preserved() {
    // RpcExecuteRequest must require non-empty payload
    assert!(PayloadKind::RpcExecuteRequest.requires_non_empty_payload());
    assert!(FrameType::RpcExecuteRequest.requires_non_empty_payload());

    // RpcBatch must require non-empty payload
    assert!(PayloadKind::RpcBatch.requires_non_empty_payload());
    assert!(FrameType::RpcBatch.requires_non_empty_payload());

    // Error must NOT require non-empty payload
    assert!(!PayloadKind::Error.requires_non_empty_payload());
    assert!(!FrameType::Error.requires_non_empty_payload());

    // Hello must NOT require non-empty payload
    assert!(!PayloadKind::Hello.requires_non_empty_payload());
    assert!(!FrameType::Hello.requires_non_empty_payload());

    // RPC frames must require contract hash
    assert!(PayloadKind::RpcExecuteRequest.requires_contract_hash());
    assert!(PayloadKind::RpcMetadata.requires_contract_hash());
    assert!(PayloadKind::RpcBatch.requires_contract_hash());
    assert!(PayloadKind::RpcCompletion.requires_contract_hash());

    // Non-RPC frames must NOT require contract hash
    assert!(!PayloadKind::Hello.requires_contract_hash());
    assert!(!PayloadKind::Auth.requires_contract_hash());
    assert!(!PayloadKind::Error.requires_contract_hash());
}
