use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Stable identifier for a permission required to invoke a procedure.
///
/// The manifest stores permissions as canonical string ids rather than a Rust
/// enum so client source generators can map them onto each language's permission
/// catalogue without re-coupling the protocol crate to the security crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredPermission {
    /// Canonical permission id, e.g. `"andromeda.execute_procedure"`. Must be
    /// non-empty, lower-case, dot-separated.
    pub id: String,
    /// Permission family id (e.g. `"application"`, `"security"`). Must be
    /// non-empty.
    pub family: String,
}

impl RequiredPermission {
    pub fn new(id: impl Into<String>, family: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            family: family.into(),
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.id.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "required permission id must not be empty",
            ));
        }

        if self.id != self.id.to_ascii_lowercase() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "required permission id must be lower-case",
            ));
        }

        if self.family.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "required permission family must not be empty",
            ));
        }

        Ok(())
    }
}
