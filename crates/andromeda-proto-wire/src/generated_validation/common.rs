use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

pub(super) fn validate_optional_catalog_version(
    label: &str,
    version: Option<u64>,
) -> AndromedaResult<()> {
    if version == Some(0) {
        return contract_error(format!("{label} must be nonzero when present"));
    }

    Ok(())
}

pub(super) fn contract_error<T>(message: impl Into<String>) -> AndromedaResult<T> {
    Err(AndromedaError::new(
        AndromedaErrorKind::Contract,
        message.into(),
    ))
}

pub(super) fn protocol_error<T>(message: impl Into<String>) -> AndromedaResult<T> {
    Err(AndromedaError::new(
        AndromedaErrorKind::Protocol,
        message.into(),
    ))
}

pub(super) fn resource_error<T>(message: impl Into<String>) -> AndromedaResult<T> {
    Err(AndromedaError::new(
        AndromedaErrorKind::Resource,
        message.into(),
    ))
}
