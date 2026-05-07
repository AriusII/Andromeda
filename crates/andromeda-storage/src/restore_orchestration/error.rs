pub(super) fn restore_error(message: impl Into<String>) -> andromeda_core::AndromedaError {
    andromeda_core::AndromedaError::new(andromeda_core::AndromedaErrorKind::Storage, message)
}
