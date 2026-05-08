use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

pub(crate) fn validate_srpl_identifier(value: &str, context: &str) -> AndromedaResult<()> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("{context} must not be empty"),
        ));
    };

    if !(first.is_ascii_alphabetic() || first == '_')
        || chars.any(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
    {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("{context} must be an ASCII identifier"),
        ));
    }

    Ok(())
}
