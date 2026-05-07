//! Compatibility reexports for WAL segment value types.
//!
//! The canonical segment types moved to `andromeda_wal::write_ahead_log::segment`.
//! Storage keeps this module as a stable facade for existing callers.

pub use andromeda_wal::write_ahead_log::segment::{WalSegment, WalSegmentDescriptor};
