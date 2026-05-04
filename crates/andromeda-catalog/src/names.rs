//! Qualified names for catalog objects.
//!
//! This module provides `QualifiedName` for hierarchical naming of catalog objects.
//! Names are typically schema-qualified (e.g., "Inventory.Product") and are normalized
//! to prevent ambiguity.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QualifiedName {
    parts: Vec<String>,
}

impl QualifiedName {
    pub fn new(parts: impl IntoIterator<Item = impl Into<String>>) -> AndromedaResult<Self> {
        let parts = parts
            .into_iter()
            .map(Into::into)
            .map(|part: String| Self::normalize_part(&part))
            .collect::<AndromedaResult<Vec<String>>>()?;

        if parts.is_empty() {
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

    fn normalize_part(raw: &str) -> AndromedaResult<String> {
        let part = raw.trim();
        if part.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "qualified name must contain non-empty parts",
            ));
        }

        if part.chars().any(char::is_whitespace) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "qualified name parts must not contain whitespace",
            ));
        }

        let mut chars = part.chars();
        let Some(first) = chars.next() else {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "qualified name must contain non-empty parts",
            ));
        };

        if !is_identifier_start(first) || chars.any(|ch| !is_identifier_continue(ch)) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "qualified name parts must use ASCII letters, digits, or underscore and start with a letter or underscore",
            ));
        }

        Ok(part.to_string())
    }
}

const fn is_identifier_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

const fn is_identifier_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
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

    #[test]
    fn qualified_name_normalizes_boundary_whitespace() {
        let name = QualifiedName::parse(" Inventory . ReserveStock ").unwrap();

        assert_eq!(name.as_catalog_path(), "Inventory.ReserveStock");
    }

    #[test]
    fn qualified_name_rejects_ambiguous_or_invalid_parts() {
        assert_eq!(
            QualifiedName::parse("Inventory.Reserve Stock")
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Catalog
        );
        assert_eq!(
            QualifiedName::parse("Inventory.reserve-stock")
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Catalog
        );
        assert_eq!(
            QualifiedName::parse("Inventory.1ReserveStock")
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Catalog
        );
    }
}
