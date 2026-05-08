use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

pub(crate) fn audit_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Internal, message)
}

pub(crate) fn non_empty_reason(reason: impl Into<String>) -> AndromedaResult<String> {
    let reason = reason.into();
    if reason.trim().is_empty() {
        return Err(audit_error(
            "audit decision evidence requires a non-empty reason",
        ));
    }

    Ok(reason)
}

pub(crate) fn non_empty_evidence(label: &str, value: impl Into<String>) -> AndromedaResult<String> {
    let value = value.into();
    if value.trim().is_empty() {
        return Err(audit_error(format!(
            "audit {label} evidence requires a non-empty value",
        )));
    }

    Ok(value)
}

pub(crate) fn contains_sensitive_marker(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    [
        "-----begin",
        "private key",
        "private_key",
        "bearer ",
        "credential=",
        "password=",
        "passwd=",
        "secret=",
        "token=",
        "authorization:",
        "x-api-key",
        "payload:",
        "payload body",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}
