//! Compatibility facade for the buffer-pool owner crate.
//!
//! Buffer-pool residency, frame lifecycle, dirty tracking, Clock eviction, and
//! WAL durability flush gates now live in `andromeda-buffer-pool`. This module
//! preserves historical `andromeda_storage::buffer_pool` imports.

pub use andromeda_buffer_pool::*;
