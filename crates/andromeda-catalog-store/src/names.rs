//! Catalog qualified-name ownership facade.
//!
//! `andromeda-contract` still owns the canonical representation. This crate
//! owns the catalog-store import boundary so `andromeda-catalog` can remain a
//! compatibility facade while downstream crates migrate to narrower owners.

pub use andromeda_contract::QualifiedName;
