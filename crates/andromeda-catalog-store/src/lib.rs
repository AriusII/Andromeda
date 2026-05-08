#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Catalog Store

Runtime-free scaffold for future catalog store ownership.

This crate exposes stable taxonomy placeholders only. It does not implement
catalog storage, WAL replay, recovery, publication, release readiness,
network or disk serialization, or any application-facing ad hoc SQL surface.

Catalog publication remains outside this crate and must not become visible
without durable WAL.
"#]

mod taxonomy;

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
