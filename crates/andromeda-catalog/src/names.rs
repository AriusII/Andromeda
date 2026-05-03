use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QualifiedName {
    parts: Vec<String>,
}

impl QualifiedName {
    pub fn new(parts: impl IntoIterator<Item=impl Into<String>>) -> AndromedaResult<Self> {
        let parts = parts.into_iter().map(Into::into).collect::<Vec<String>>();

        if parts.is_empty() || parts.iter().any(|part| part.trim().is_empty()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "qualified name must contain non-empty parts",
            ));
        }

        Ok(Self { parts })
    }

    pub fn parse(value: &str) -> AndromedaResult<Self> {
        Self::new(value.split('.'))
    }

    pub fn parts(&self) -> &[String] {
        &self.parts
    }

    pub fn as_catalog_path(&self) -> String {
        self.parts.join(".")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualified_name_rejects_empty_parts() {
        assert!(QualifiedName::parse("Inventory.Product").is_ok());
        assert_eq!(
            QualifiedName::parse("Inventory..Product")
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Catalog
        );
    }
}
