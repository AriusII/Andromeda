//! Protocol version management and validation.
//!
//! This module manages protocol version compatibility, ensuring that client and server
//! communicate using compatible versions of the wire format.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolVersion {
    pub major: u32,
    pub minor: u32,
}

impl ProtocolVersion {
    pub const V1: Self = Self { major: 1, minor: 0 };
    pub const SUPPORTED_MAJOR: u32 = Self::V1.major;
    pub const SUPPORTED_MINOR: u32 = Self::V1.minor;

    pub const fn is_supported(self) -> bool {
        self.major == Self::SUPPORTED_MAJOR && self.minor == Self::SUPPORTED_MINOR
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.major == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "protocol major version must be positive",
            ));
        }

        if self.major != Self::SUPPORTED_MAJOR {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "unsupported protocol major version",
            ));
        }

        if self.minor > Self::SUPPORTED_MINOR {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "unsupported protocol minor version",
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_accepts_only_locked_v1_0_contract() {
        assert!(ProtocolVersion::V1.validate().is_ok());
        assert!(ProtocolVersion::V1.is_supported());

        assert_eq!(
            ProtocolVersion { major: 1, minor: 1 }
                .validate()
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );

        assert_eq!(
            ProtocolVersion { major: 2, minor: 0 }
                .validate()
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
    }
}
