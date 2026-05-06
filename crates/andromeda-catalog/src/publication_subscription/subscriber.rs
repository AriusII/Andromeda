use andromeda_core::AndromedaResult;

use super::catalog_publication_error;

/// Stable identifier for an administrative/HA catalog subscriber.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogSubscriberId(String);

impl CatalogSubscriberId {
    pub fn new(value: impl Into<String>) -> AndromedaResult<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return catalog_publication_error("catalog subscriber id must not be empty");
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
