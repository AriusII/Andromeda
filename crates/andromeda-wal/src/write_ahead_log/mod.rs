//! Write-ahead log domain facade for pure WAL types.
//!
//! This crate intentionally excludes storage-owned recovery implementations:
//! heap redo, catalog bridges, recovery reports, startup planning, commit log,
//! backup, HA/DR runtime, and page types. The physical FileWal byte contract is
//! re-exported here as a WAL owner surface.

pub mod codec;
pub mod durability_fence;
pub mod file;
pub mod manager;
pub mod record;
pub mod record_bounds;
pub mod segment;
pub mod transaction;

pub use codec::*;
pub use durability_fence::*;
pub use file::*;
pub use manager::*;
pub use record::*;
pub use record_bounds::*;
pub use segment::*;
pub use transaction::*;
