//! Catalog object descriptor import boundary.
//!
//! Procedure identity lives in `andromeda-procedure-contract`; catalog object
//! descriptors and bindings still live in `andromeda-contract`.

pub use andromeda_contract::{
    CatalogBindingKind, CatalogDefinition, CatalogObjectBinding, EnumDefinition, EnumVariant,
    StructuredObjectDefinition, TableDefinition,
};
pub use andromeda_procedure_contract::{CatalogObjectRef, ObjectKind};
