mod admin;
mod identity;
mod scope;
mod security;

pub use admin::*;
pub use identity::*;
pub use scope::*;
pub use security::*;

use super::{contains_sensitive_marker, non_empty_reason, observe_error};

fn non_empty_evidence(
    label: &str,
    value: impl Into<String>,
) -> andromeda_error::AndromedaResult<String> {
    let value = value.into();
    if value.trim().is_empty() {
        return Err(observe_error(format!(
            "observability {label} evidence requires a non-empty value",
        )));
    }

    Ok(value)
}

#[cfg(test)]
mod tests;
