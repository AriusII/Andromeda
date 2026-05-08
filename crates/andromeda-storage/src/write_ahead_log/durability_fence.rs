//! Compatibility reexports for WAL durability fence checks.
//!
//! The canonical LSN-ordering fence logic moved to
//! `andromeda_wal::write_ahead_log::durability_fence`. Storage keeps this shim
//! so existing durability validation import paths continue to resolve during
//! migration.

pub use andromeda_wal::{
    DurabilityFenceError, validate_lsn_ordered, validate_lsn_strictly_ordered,
    validate_manifest_atomic_switch, validate_recovery_floor,
    validate_wal_durability_before_page_flush,
};

#[cfg(test)]
mod tests;
