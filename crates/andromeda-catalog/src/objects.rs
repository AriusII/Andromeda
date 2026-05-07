//! Temporary compatibility facade for catalog object descriptors.
//!
//! Lot 2.1 moves contract-safe object descriptors to `andromeda-contract`.
//! Catalog storage, WAL codecs, snapshots, and DefinitionBatch logic continue
//! to live in `andromeda-catalog`.

pub use andromeda_contract::{
    CatalogBindingKind, CatalogDefinition, CatalogObjectBinding, CatalogObjectRef, EnumDefinition,
    EnumVariant, ObjectKind, StructuredObjectDefinition, TableDefinition,
};
