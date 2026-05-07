use andromeda_catalog::QualifiedName;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

pub(super) fn validate_qualified_name(name: &QualifiedName, context: &str) -> AndromedaResult<()> {
    if name.parts().is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("{context} must not be empty"),
        ));
    }

    Ok(())
}
