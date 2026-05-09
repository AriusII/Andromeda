//! Protocol drift detection and invariant validation.

use crate::{
    AUTH_FRAME_CODE, CONTRACT_REQUEST_FRAME_CODE, CONTRACT_RESPONSE_FRAME_CODE, ERROR_FRAME_CODE,
    FRAME_CODEC_CRC_OFFSET, FRAME_CODEC_HEADER_LEN, FrameHeader, HELLO_FRAME_CODE,
    RPC_BATCH_FRAME_CODE, RPC_COMPLETION_FRAME_CODE, RPC_EXECUTE_REQUEST_FRAME_CODE,
    RPC_METADATA_FRAME_CODE, TELEMETRY_SOFT_SIGNAL_FRAME_CODE,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::ops::RangeInclusive;

const _: () = assert!(FRAME_CODEC_HEADER_LEN == 52);
const _: () = assert!(FRAME_CODEC_CRC_OFFSET == 48);

pub const fn assert_frame_header_layout() {}

/// Validates that the encoded frame header contract remains stable.
pub fn validate_frame_header_layout() -> AndromedaResult<()> {
    assert_frame_header_layout();
    ensure_usize(FRAME_CODEC_HEADER_LEN, 52, "FRAME_CODEC_HEADER_LEN")?;
    ensure_usize(FRAME_CODEC_CRC_OFFSET, 48, "FRAME_CODEC_CRC_OFFSET")
}

/// Frame type wire code invariants.
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

    const VALIDATION: &'static [(u32, u32, &'static str)] = &[
        (HELLO_FRAME_CODE, 1, "HELLO_FRAME_CODE"),
        (AUTH_FRAME_CODE, 2, "AUTH_FRAME_CODE"),
        (
            CONTRACT_REQUEST_FRAME_CODE,
            3,
            "CONTRACT_REQUEST_FRAME_CODE",
        ),
        (
            CONTRACT_RESPONSE_FRAME_CODE,
            4,
            "CONTRACT_RESPONSE_FRAME_CODE",
        ),
        (
            RPC_EXECUTE_REQUEST_FRAME_CODE,
            5,
            "RPC_EXECUTE_REQUEST_FRAME_CODE",
        ),
        (RPC_METADATA_FRAME_CODE, 6, "RPC_METADATA_FRAME_CODE"),
        (RPC_BATCH_FRAME_CODE, 7, "RPC_BATCH_FRAME_CODE"),
        (RPC_COMPLETION_FRAME_CODE, 8, "RPC_COMPLETION_FRAME_CODE"),
        (ERROR_FRAME_CODE, 9, "ERROR_FRAME_CODE"),
        (
            TELEMETRY_SOFT_SIGNAL_FRAME_CODE,
            100,
            "TELEMETRY_SOFT_SIGNAL_FRAME_CODE",
        ),
    ];

    /// Validates that frame type codes remain locked.
    pub fn validate() -> AndromedaResult<()> {
        for (actual, expected, name) in Self::VALIDATION {
            ensure_u32(*actual, *expected, name)?;
        }
        Ok(())
    }
}

/// Payload kind wire code invariants.
pub struct PayloadKindInvariants;

impl PayloadKindInvariants {
    /// Locked payload kind discriminator values, including PAYLOAD_ERROR=9.
    pub const LOCKED_RANGE: RangeInclusive<u32> = 1..=9;

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

    const VALIDATION: &'static [(u32, u32, &'static str)] = &[
        (Self::PAYLOAD_HELLO, 1, "PAYLOAD_HELLO"),
        (Self::PAYLOAD_AUTH, 2, "PAYLOAD_AUTH"),
        (
            Self::PAYLOAD_CONTRACT_REQUEST,
            3,
            "PAYLOAD_CONTRACT_REQUEST",
        ),
        (
            Self::PAYLOAD_CONTRACT_RESPONSE,
            4,
            "PAYLOAD_CONTRACT_RESPONSE",
        ),
        (
            Self::PAYLOAD_RPC_EXECUTE_REQUEST,
            5,
            "PAYLOAD_RPC_EXECUTE_REQUEST",
        ),
        (Self::PAYLOAD_RPC_METADATA, 6, "PAYLOAD_RPC_METADATA"),
        (Self::PAYLOAD_RPC_BATCH, 7, "PAYLOAD_RPC_BATCH"),
        (Self::PAYLOAD_RPC_COMPLETION, 8, "PAYLOAD_RPC_COMPLETION"),
        (Self::PAYLOAD_ERROR, 9, "PAYLOAD_ERROR"),
    ];

    /// Validates that PayloadKind discriminators remain locked in range [1..9].
    pub fn validate() -> AndromedaResult<()> {
        ensure_u32(
            *Self::LOCKED_RANGE.start(),
            1,
            "PAYLOAD_KIND_LOCKED_RANGE_START",
        )?;
        ensure_u32(
            *Self::LOCKED_RANGE.end(),
            9,
            "PAYLOAD_KIND_LOCKED_RANGE_END",
        )?;
        for (actual, expected, name) in Self::VALIDATION {
            ensure_u32(*actual, *expected, name)?;
        }
        Ok(())
    }
}

/// Protocol version invariants.
pub struct ProtocolVersionInvariants;

impl ProtocolVersionInvariants {
    /// Current locked protocol version (V1.0).
    pub const LOCKED_MAJOR: u32 = 1;
    pub const LOCKED_MINOR: u32 = 0;

    /// Validates that protocol version is locked at V1.0.
    pub fn validate() -> AndromedaResult<()> {
        ensure_u32(Self::LOCKED_MAJOR, 1, "PROTOCOL_LOCKED_MAJOR")?;
        ensure_u32(Self::LOCKED_MINOR, 0, "PROTOCOL_LOCKED_MINOR")
    }
}

/// Master validator for all protocol invariants.
pub struct ProtocolInvariants;

impl ProtocolInvariants {
    /// Runs all protocol invariant validations.
    ///
    /// Should be called once at module initialization or during
    /// connection setup to catch any protocol drift early.
    pub fn validate_all() -> AndromedaResult<()> {
        validate_frame_header_layout()?;
        FrameTypeInvariants::validate()?;
        PayloadKindInvariants::validate()?;
        ProtocolVersionInvariants::validate()?;

        Ok(())
    }

    /// Returns a diagnostic report of all protocol invariants.
    pub fn diagnostic_report() -> String {
        let mut report = String::new();

        report.push_str("=== Protocol Invariants Diagnostic Report ===\n\n");

        report.push_str("Frame Header Layout:\n");
        report.push_str(&format!(
            "  Size: {} bytes (expected 52)\n",
            size_of::<FrameHeader>()
        ));
        report.push_str(&format!(
            "  Frame Type Code Invariants: {} items locked\n",
            FrameTypeInvariants::LOCKED_CODES.len()
        ));

        report.push_str("\nFrame Type Codes:\n");
        for (code, name) in FrameTypeInvariants::EXPECTED {
            report.push_str(&format!("  {}: {}\n", name, code));
        }

        report.push_str("\nPayload Kind Discriminators:\n");
        report.push_str(&format!(
            "  Range: {}..{}\n",
            PayloadKindInvariants::LOCKED_RANGE.start(),
            PayloadKindInvariants::LOCKED_RANGE.end()
        ));

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

fn ensure_u32(actual: u32, expected: u32, name: &'static str) -> AndromedaResult<()> {
    if actual == expected {
        return Ok(());
    }

    Err(AndromedaError::new(
        AndromedaErrorKind::Protocol,
        format!("{name} changed! Expected {expected}, got {actual}"),
    ))
}

fn ensure_usize(actual: usize, expected: usize, name: &'static str) -> AndromedaResult<()> {
    if actual == expected {
        return Ok(());
    }

    Err(AndromedaError::new(
        AndromedaErrorKind::Protocol,
        format!("{name} changed! Expected {expected}, got {actual}"),
    ))
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
        assert!(
            PayloadKindInvariants::LOCKED_RANGE.contains(&PayloadKindInvariants::PAYLOAD_ERROR)
        );
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
