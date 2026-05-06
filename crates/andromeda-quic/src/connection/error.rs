use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub(super) fn protocol_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}
