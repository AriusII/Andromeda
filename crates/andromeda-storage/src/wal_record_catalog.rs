//! Compatibility exports for storage-facing catalog WAL record contracts.
//!
//! The record family is owned by `andromeda-catalog-recovery`; storage exposes
//! the historical names while runtime callers migrate to the owner crate.

pub use andromeda_catalog_recovery::{
    CatalogStorageWalRecord as CatalogWalRecord,
    CatalogStorageWalRecordVersion as CatalogWalRecordVersion,
};
