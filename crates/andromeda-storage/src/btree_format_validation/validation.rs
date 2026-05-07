use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

pub(super) fn validate_identity_parts(
    major: u32,
    minor: u32,
    max_key_size: u16,
) -> AndromedaResult<()> {
    if major == 0 && minor == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "B-Tree key format version 0.0 is reserved",
        ));
    }
    if max_key_size == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "B-Tree key format max_key_size must not be zero",
        ));
    }
    Ok(())
}
