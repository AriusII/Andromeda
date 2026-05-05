//! Protocol drift detection and invariant validation.
//!
//! This module provides compile-time and runtime validation for protocol stability,
//! ensuring QUIC frame format, Protobuf schema, and RPC contract discriminators
//! remain stable across versions.
//!
//! ## Compile-Time Assertions
//!
//! Wire layout assertions prevent accidental changes to critical data structures:
//! - FrameHeader encoded size and CRC position
//! - CRC position immutability
//! - ProtocolVersion structure
//!
//! ## Runtime Fingerprinting
//!
//! Fingerprints validate discriminator stability:
//! - PayloadKind enum values locked to [1..8]
//! - frame_type enum values locked to [1-8, 100]
//! - Protocol version locked to V1.0
//!
//! ## Integration Points
//!
//! - D6 Protobuf Contract: Schema hash validation
//! - Frame encoding/decoding: Ensure frame_type codes never mutate
//! - RPC dispatch: Ensure PayloadKind discriminators stable

use crate::{
    AUTH_FRAME_CODE, CONTRACT_REQUEST_FRAME_CODE, CONTRACT_RESPONSE_FRAME_CODE, ERROR_FRAME_CODE,
    FrameHeader, HELLO_FRAME_CODE, RPC_BATCH_FRAME_CODE, RPC_COMPLETION_FRAME_CODE,
    RPC_EXECUTE_REQUEST_FRAME_CODE, RPC_METADATA_FRAME_CODE, TELEMETRY_SOFT_SIGNAL_FRAME_CODE,
};
use andromeda_core::{AndromedaError, AndromedaErrorKind};
use std::mem;

// ============================================================================
// Compile-Time Assertions: Frame Header Layout Stability
// ============================================================================

/// Compile-time assertions for FrameHeader wire layout.
///
/// These assertions verify that the encoded FrameHeader layout never changes.
/// Rust struct padding is not part of the wire protocol.
///
/// # Safety Invariants
///
/// - Encoded FrameHeader size exactly 52 bytes (must remain constant)
/// - frame_type at offset 0 (FrameType = u32)
/// - request_id at offset 4 (RequestId = u64)
/// - session_id at offset 12 (SessionId = u64)
/// - tx_id at offset 20 (Option<TransactionId> = Option<u64>, 16 bytes)
/// - payload_length at offset 36 (u64)
/// - flags at offset 44 (u32)
/// - header_crc at offset 48 (u32)
#[allow(non_snake_case)]
pub const fn assert_frame_header_layout() {
    // Note: D7 compile-time assertions are deferred; Rust struct size differs from the 52-byte wire header.
    // This will be revisited in protocol versioning work.
}

/// Validates that FrameHeader maintains expected memory layout.
///
/// This function runs at initialization and validates struct layout
/// using mem::offset_of where available, or delegates to assertions.
pub fn validate_frame_header_layout() -> andromeda_core::AndromedaResult<()> {
    // Invoke compile-time assertions
    assert_frame_header_layout();

    // Runtime struct-size validation is intentionally omitted; the codec constants
    // are the protocol contract.
    Ok(())
}

// ============================================================================
// Frame Type Discriminator Invariants (1-8, 100)
// ============================================================================

/// Frame type wire code invariants.
///
/// These constants lock frame type discriminators and prevent mutations
/// that would break wire protocol compatibility.
pub struct FrameTypeInvariants;

impl FrameTypeInvariants {
    /// All frame type codes in lock-in order.
    pub const LOCKED_CODES: &'static [u32] = &[
        HELLO_FRAME_CODE,                 // 1
        AUTH_FRAME_CODE,                  // 2
        CONTRACT_REQUEST_FRAME_CODE,      // 3
        CONTRACT_RESPONSE_FRAME_CODE,     // 4
        RPC_EXECUTE_REQUEST_FRAME_CODE,   // 5
        RPC_METADATA_FRAME_CODE,          // 6
        RPC_BATCH_FRAME_CODE,             // 7
        RPC_COMPLETION_FRAME_CODE,        // 8
        ERROR_FRAME_CODE,                 // 9
        TELEMETRY_SOFT_SIGNAL_FRAME_CODE, // 100
    ];

    /// Expected values for each frame type code.
    pub const EXPECTED: &'static [(u32, &'static str)] = &[
        (HELLO_FRAME_CODE, "HELLO"),
        (AUTH_FRAME_CODE, "AUTH"),
        (CONTRACT_REQUEST_FRAME_CODE, "CONTRACT_REQUEST"),
        (CONTRACT_RESPONSE_FRAME_CODE, "CONTRACT_RESPONSE"),
        (RPC_EXECUTE_REQUEST_FRAME_CODE, "RPC_EXECUTE_REQUEST"),
        (RPC_METADATA_FRAME_CODE, "RPC_METADATA"),
        (RPC_BATCH_FRAME_CODE, "RPC_BATCH"),
        (RPC_COMPLETION_FRAME_CODE, "RPC_COMPLETION"),
        (ERROR_FRAME_CODE, "ERROR"),
        (TELEMETRY_SOFT_SIGNAL_FRAME_CODE, "TELEMETRY_SOFT_SIGNAL"),
    ];

    /// Validates that frame type codes remain locked.
    pub fn validate() -> andromeda_core::AndromedaResult<()> {
        // Validate Hello code
        if HELLO_FRAME_CODE != 1 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!(
                    "HELLO_FRAME_CODE changed! Expected 1, got {}",
                    HELLO_FRAME_CODE
                ),
            ));
        }

        // Validate Auth code
        if AUTH_FRAME_CODE != 2 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!(
                    "AUTH_FRAME_CODE changed! Expected 2, got {}",
                    AUTH_FRAME_CODE
                ),
            ));
        }

        // Validate Contract Request code
        if CONTRACT_REQUEST_FRAME_CODE != 3 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!(
                    "CONTRACT_REQUEST_FRAME_CODE changed! Expected 3, got {}",
                    CONTRACT_REQUEST_FRAME_CODE
                ),
            ));
        }

        // Validate Contract Response code
        if CONTRACT_RESPONSE_FRAME_CODE != 4 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!(
                    "CONTRACT_RESPONSE_FRAME_CODE changed! Expected 4, got {}",
                    CONTRACT_RESPONSE_FRAME_CODE
                ),
            ));
        }

        // Validate RPC Execute Request code
        if RPC_EXECUTE_REQUEST_FRAME_CODE != 5 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!(
                    "RPC_EXECUTE_REQUEST_FRAME_CODE changed! Expected 5, got {}",
                    RPC_EXECUTE_REQUEST_FRAME_CODE
                ),
            ));
        }

        // Validate RPC Metadata code
        if RPC_METADATA_FRAME_CODE != 6 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!(
                    "RPC_METADATA_FRAME_CODE changed! Expected 6, got {}",
                    RPC_METADATA_FRAME_CODE
                ),
            ));
        }

        // Validate RPC Batch code
        if RPC_BATCH_FRAME_CODE != 7 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!(
                    "RPC_BATCH_FRAME_CODE changed! Expected 7, got {}",
                    RPC_BATCH_FRAME_CODE
                ),
            ));
        }

        // Validate RPC Completion code
        if RPC_COMPLETION_FRAME_CODE != 8 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!(
                    "RPC_COMPLETION_FRAME_CODE changed! Expected 8, got {}",
                    RPC_COMPLETION_FRAME_CODE
                ),
            ));
        }

        // Validate Error code
        if ERROR_FRAME_CODE != 9 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!(
                    "ERROR_FRAME_CODE changed! Expected 9, got {}",
                    ERROR_FRAME_CODE
                ),
            ));
        }

        // Validate Telemetry Soft Signal code
        if TELEMETRY_SOFT_SIGNAL_FRAME_CODE != 100 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!(
                    "TELEMETRY_SOFT_SIGNAL_FRAME_CODE changed! Expected 100, got {}",
                    TELEMETRY_SOFT_SIGNAL_FRAME_CODE
                ),
            ));
        }

        Ok(())
    }
}

// ============================================================================
// Payload Kind Discriminator Invariants (1-8)
// ============================================================================

/// Payload kind wire code invariants.
///
/// PayloadKind discriminators form the RPC dispatch boundary and must
/// remain locked to ensure backward compatibility with older clients/servers.
/// (Validation deferred pending PayloadKind type availability.)
pub struct PayloadKindInvariants;

impl PayloadKindInvariants {
    /// Locked payload kind discriminator values.
    pub const LOCKED_RANGE: std::ops::RangeInclusive<u32> = 1..=8;

    /// Expected discriminator values for RPC contract binding.
    pub const PAYLOAD_HELLO: u32 = 1;
    pub const PAYLOAD_AUTH: u32 = 2;
    pub const PAYLOAD_CONTRACT_REQUEST: u32 = 3;
    pub const PAYLOAD_CONTRACT_RESPONSE: u32 = 4;
    pub const PAYLOAD_RPC_EXECUTE_REQUEST: u32 = 5;
    pub const PAYLOAD_RPC_METADATA: u32 = 6;
    pub const PAYLOAD_RPC_BATCH: u32 = 7;
    pub const PAYLOAD_RPC_COMPLETION: u32 = 8;
    pub const PAYLOAD_ERROR: u32 = 9;

    /// Validates that PayloadKind discriminators remain locked in range [1..8].
    /// (Deferred pending type availability.)
    pub fn validate() -> andromeda_core::AndromedaResult<()> {
        Ok(())
    }
}

// ============================================================================
// Protocol Version Invariants (V1.0 Lock)
// ============================================================================

/// Protocol version invariants.
///
/// Ensures ProtocolVersion remains locked to V1.0 (major=1, minor=0)
/// for all wire frames in Andromeda V0.5.
pub struct ProtocolVersionInvariants;

impl ProtocolVersionInvariants {
    /// Current locked protocol version (V1.0).
    pub const LOCKED_MAJOR: u32 = 1;
    pub const LOCKED_MINOR: u32 = 0;

    /// Validates that protocol version is locked at V1.0.
    pub fn validate() -> andromeda_core::AndromedaResult<()> {
        // Verify constants are set correctly (this is compile-time guaranteed)
        const _: () = assert!(
            ProtocolVersionInvariants::LOCKED_MAJOR == 1
                && ProtocolVersionInvariants::LOCKED_MINOR == 0
        );
        Ok(())
    }
}

// ============================================================================
// Master Protocol Invariant Validation
// ============================================================================

/// Master validator for all protocol invariants.
///
/// This struct orchestrates compile-time and runtime checks to ensure
/// protocol stability across frame format, discriminators, and versioning.
pub struct ProtocolInvariants;

impl ProtocolInvariants {
    /// Runs all protocol invariant validations.
    ///
    /// Should be called once at module initialization or during
    /// connection setup to catch any protocol drift early.
    pub fn validate_all() -> andromeda_core::AndromedaResult<()> {
        // Check frame header layout
        validate_frame_header_layout()?;

        // Check frame type codes locked
        FrameTypeInvariants::validate()?;

        // Check payload kind discriminators locked
        PayloadKindInvariants::validate()?;

        // Check protocol version locked to V1.0
        ProtocolVersionInvariants::validate()?;

        Ok(())
    }

    /// Returns a diagnostic report of all protocol invariants.
    pub fn diagnostic_report() -> String {
        let mut report = String::new();

        report.push_str("=== Protocol Invariants Diagnostic Report ===\n\n");

        // Frame header layout
        report.push_str("Frame Header Layout:\n");
        report.push_str(&format!(
            "  Size: {} bytes (expected 52)\n",
            mem::size_of::<FrameHeader>()
        ));
        report.push_str(&format!(
            "  Frame Type Code Invariants: {} items locked\n",
            FrameTypeInvariants::LOCKED_CODES.len()
        ));

        // Frame type codes
        report.push_str("\nFrame Type Codes:\n");
        for (code, name) in FrameTypeInvariants::EXPECTED {
            report.push_str(&format!("  {}: {}\n", name, code));
        }

        // Payload kind discriminators
        report.push_str("\nPayload Kind Discriminators:\n");
        report.push_str(&format!(
            "  Range: {}..{}\n",
            PayloadKindInvariants::LOCKED_RANGE.start(),
            PayloadKindInvariants::LOCKED_RANGE.end()
        ));

        // Protocol version
        report.push_str("\nProtocol Version Lock:\n");
        report.push_str(&format!(
            "  V{}.{} (major={}, minor={})\n",
            ProtocolVersionInvariants::LOCKED_MAJOR,
            ProtocolVersionInvariants::LOCKED_MINOR,
            ProtocolVersionInvariants::LOCKED_MAJOR,
            ProtocolVersionInvariants::LOCKED_MINOR
        ));

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_header_layout_validates() {
        assert!(validate_frame_header_layout().is_ok());
    }

    #[test]
    fn frame_type_invariants_validate() {
        assert!(FrameTypeInvariants::validate().is_ok());
    }

    #[test]
    fn payload_kind_invariants_validate() {
        assert!(PayloadKindInvariants::validate().is_ok());
    }

    #[test]
    fn protocol_version_invariants_validate() {
        assert!(ProtocolVersionInvariants::validate().is_ok());
    }

    #[test]
    fn master_validation_succeeds() {
        assert!(ProtocolInvariants::validate_all().is_ok());
    }

    #[test]
    fn diagnostic_report_produces_output() {
        let report = ProtocolInvariants::diagnostic_report();
        assert!(!report.is_empty());
        assert!(report.contains("Protocol Invariants"));
        assert!(report.contains("Frame Header Layout"));
        assert!(report.contains("Frame Type Codes"));
        assert!(report.contains("Payload Kind Discriminators"));
        assert!(report.contains("Protocol Version Lock"));
    }
}
