//! Write-ahead log domain facade for pure WAL types.
//!
//! This crate intentionally excludes storage-owned runtime implementations:
//! file-backed WAL, heap redo, catalog bridges, recovery, commit log,
//! backup, HA/DR runtime, and page types.

pub mod codec;
pub mod durability_fence;
pub mod manager;
pub mod record;
pub mod record_bounds;
pub mod segment;
pub mod transaction;

pub use codec::*;
pub use durability_fence::*;
pub use manager::*;
pub use record::*;
pub use record_bounds::*;
pub use segment::*;
pub use transaction::*;
