//! CLI error handling.
//!
//! Provides user-friendly error creation and handling for CLI operations.

use andromeda_error::{AndromedaError, AndromedaErrorKind};

/// Creates a CLI error with protocol error kind.
pub fn cli_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

/// Creates a protocol validation error.
pub fn protocol_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_error_creates_protocol_error() {
        let err = cli_error("test message");
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    }

    #[test]
    fn protocol_error_creates_protocol_error() {
        let err = protocol_error("test message");
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    }
}
