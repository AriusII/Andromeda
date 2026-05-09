#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Contract Compat

Runtime-free owner for Procedure contract compatibility taxonomy identifiers.

This crate exposes stable reserved taxonomy identifiers only. It does not evaluate
compatibility, publish catalog changes, claim release readiness, serialize
network or disk formats, or authorize any application-facing ad hoc SQL surface.

Catalog publication remains outside this crate and must not become visible
without durable WAL.
"#]

mod taxonomy;

pub use taxonomy::{
    ALL_CONTRACT_COMPATIBILITY_CLASSES, CONTRACT_COMPAT_COMPATIBLE_ADDITIVE,
    CONTRACT_COMPAT_COMPATIBLE_EXACT, CONTRACT_COMPAT_COMPATIBLE_METADATA_ONLY,
    CONTRACT_COMPAT_INCOMPATIBLE_PERMISSION, CONTRACT_COMPAT_INCOMPATIBLE_SHAPE,
    CONTRACT_COMPAT_REVIEW_REQUIRED, CONTRACT_COMPAT_SCHEMA_VERSION, CONTRACT_COMPAT_TAXONOMY_ID,
    ContractCompatibilityClass, TaxonomyEntry, TaxonomyStatus,
};
