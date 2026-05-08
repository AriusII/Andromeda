use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

pub(crate) fn checked_add(lhs: u32, rhs: u32, context: &'static str) -> AndromedaResult<u32> {
    lhs.checked_add(rhs)
        .ok_or_else(|| storage_error(format!("{context} overflows u32")))
}

pub(crate) fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
