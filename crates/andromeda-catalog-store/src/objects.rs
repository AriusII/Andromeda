//! Catalog object descriptor ownership re-export surface.
//!
//! `andromeda-contract` owns the canonical descriptor implementations and
//! validation. This crate owns the catalog-store import boundary for object
//! identities, definitions, and bindings.

pub use andromeda_contract::{
    CatalogBindingKind, CatalogDefinition, CatalogObjectBinding, CatalogObjectRef, EnumDefinition,
    EnumVariant, ObjectKind, StructuredObjectDefinition, TableDefinition,
};
