use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub(super) fn protocol_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

pub(super) fn contract_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Contract, message)
}

pub(super) fn security_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Security, message)
}
