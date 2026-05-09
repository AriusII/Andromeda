use andromeda_error::{AndromedaError, AndromedaErrorKind};

pub(crate) fn reconnect_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

pub(crate) fn pool_error(kind: AndromedaErrorKind, message: &'static str) -> AndromedaError {
    AndromedaError::new(kind, message)
}
