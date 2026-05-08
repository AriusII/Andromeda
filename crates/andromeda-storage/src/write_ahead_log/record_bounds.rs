//! Compatibility reexports for WAL record and batch bounds.
//!
//! The canonical bounded validation logic moved to
//! `andromeda_wal::write_ahead_log::record_bounds`. Storage keeps this shim so
//! older import paths continue to resolve during migration.

pub use andromeda_wal::{
    WAL_BATCH_ROW_LIMIT, WAL_RECORD_HEADER_OVERHEAD, WAL_RECORD_SIZE_LIMIT, WAL_SEGMENT_BOUNDARY,
    validate_lsn_continuity, validate_record_size, validate_segment_boundary,
    validate_transaction_batch_cardinality, validate_wal_batch_bounds, validate_wal_record_bounds,
};

#[cfg(test)]
mod tests;
