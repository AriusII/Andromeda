#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Catalog Store

Runtime-free catalog object and store boundary crate.

This crate owns the narrow import boundary for catalog object descriptors,
qualified names, store-facing report DTOs, append-sequence validation, and
stable taxonomy placeholders. It does not implement catalog storage, WAL replay,
recovery, publication, release readiness, network or disk serialization, or any
application-facing ad hoc SQL surface.

Catalog publication remains outside this crate and must not become visible
without durable WAL.
"#]

mod names;
mod objects;
mod store_boundary;
mod taxonomy;

pub use names::QualifiedName;
pub use objects::{
    CatalogBindingKind, CatalogDefinition, CatalogObjectBinding, CatalogObjectRef, EnumDefinition,
    EnumVariant, ObjectKind, StructuredObjectDefinition, TableDefinition,
};
pub use store_boundary::{
    CatalogStoreApplyReport, CatalogStoreDurableApplyReport, CatalogStoreMutationKind,
    CatalogStoreWalAppend, CatalogStoreWalAppendSequenceError,
    validate_catalog_store_wal_append_sequence,
};
pub use taxonomy::{
    ALL_CATALOG_STORE_BARRIERS, ALL_CATALOG_STORE_RESPONSIBILITIES,
    ALL_CATALOG_STORE_SNAPSHOT_CLASSES, CATALOG_STORE_BARRIER_AUDIT_TRACE_REQUIRED,
    CATALOG_STORE_BARRIER_DURABLE_WAL_REQUIRED, CATALOG_STORE_BARRIER_RECOVERY_VALIDATED,
    CATALOG_STORE_RESPONSIBILITY_OBJECT_LOOKUP, CATALOG_STORE_RESPONSIBILITY_ROOT_POINTER,
    CATALOG_STORE_RESPONSIBILITY_VERSION_HISTORY, CATALOG_STORE_SCHEMA_VERSION,
    CATALOG_STORE_SNAPSHOT_CONSISTENT_READ, CATALOG_STORE_SNAPSHOT_FORENSIC_READ,
    CATALOG_STORE_SNAPSHOT_VISIBLE_CATALOG, CATALOG_STORE_TAXONOMY_ID, CatalogStoreBarrier,
    CatalogStoreResponsibility, CatalogStoreSnapshotClass, TaxonomyEntry, TaxonomyStatus,
};
