#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Catalog Diff

Runtime-free catalog object diff boundary crate.

This crate owns object-level diff evidence and stable taxonomy placeholders. It
does not evaluate Procedure compatibility, apply changes, publish catalog state,
claim release readiness, serialize network or disk formats, or authorize any
application-facing ad hoc SQL surface.

Catalog publication remains outside this crate and must not become visible
without durable WAL.
"#]

mod object_diff;
mod taxonomy;

pub use object_diff::{
    CatalogObjectDiff, CatalogObjectDiffImpact, CatalogObjectDiffKind, CatalogObjectDiffSeverity,
    diff_catalog_object_definitions,
};
pub use taxonomy::{
    ALL_CATALOG_DIFF_IMPACT_KINDS, ALL_CATALOG_DIFF_KINDS, ALL_CATALOG_DIFF_SEVERITIES,
    CATALOG_DIFF_IMPACT_CONTRACT_HASH_CHANGED, CATALOG_DIFF_IMPACT_DEPENDENCY_CHANGED,
    CATALOG_DIFF_IMPACT_PERMISSION_CHANGED, CATALOG_DIFF_KIND_OBJECT_ADDED,
    CATALOG_DIFF_KIND_OBJECT_REMOVED, CATALOG_DIFF_KIND_OBJECT_REPLACED,
    CATALOG_DIFF_SCHEMA_VERSION, CATALOG_DIFF_SEVERITY_BREAKING_REVIEW,
    CATALOG_DIFF_SEVERITY_INFORMATIONAL, CATALOG_DIFF_SEVERITY_WAL_REQUIRED,
    CATALOG_DIFF_TAXONOMY_ID, CatalogDiffImpactKind, CatalogDiffKind, CatalogDiffSeverity,
    TaxonomyEntry, TaxonomyStatus,
};
