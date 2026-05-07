use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub(crate) fn encoder_error(msg: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, msg)
}
