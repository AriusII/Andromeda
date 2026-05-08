//! Compatibility facade for catalog object descriptors.
//!
//! `andromeda-catalog-store` now owns the catalog-store import boundary for
//! object descriptors. The canonical definitions still live in
//! `andromeda-contract`; this module preserves the legacy
//! `andromeda_catalog::*` facade.

pub use andromeda_catalog_store::{
    CatalogBindingKind, CatalogDefinition, CatalogObjectBinding, CatalogObjectRef, EnumDefinition,
    EnumVariant, ObjectKind, StructuredObjectDefinition, TableDefinition,
};
pub use andromeda_contract::{
    compute_structured_object_shape_hash, encode_structured_object_shape_material,
    structured_object_shape_hash_compatible, validate_structured_object_shape,
};
