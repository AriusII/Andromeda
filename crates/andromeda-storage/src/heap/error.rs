use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub(crate) fn heap_error(msg: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, msg)
}
