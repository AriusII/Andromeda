//! WAL durability observer for LSN-safe dirty flush gate.
//!
//! The owner boundary lives in `andromeda-buffer-pool`. This module remains a
//! compatibility reexport for historical `andromeda_storage::buffer_pool`
//! imports while the resident frame implementation is still storage-owned.

pub use andromeda_buffer_pool::{TestWalDurabilityObserver, WalDurabilityObserver};
