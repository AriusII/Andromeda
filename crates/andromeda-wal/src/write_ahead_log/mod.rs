//! Write-ahead log domain facade for pure WAL types.
//!
//! This crate intentionally excludes storage-owned recovery implementations:
//! heap redo, catalog bridges, recovery reports, startup planning, commit log,
//! backup, HA/DR runtime, and page types. The physical FileWal byte contract is
//! re-exported here as a WAL owner surface.

pub mod codec;
pub mod commit_log_entry;
pub mod commit_log_facade;
pub mod compaction;
pub mod durability_fence;
pub mod file;
pub mod gc;
pub mod gc_eligibility;
pub mod manager;
pub mod record;
pub mod record_bounds;
pub mod segment;
pub mod segment_reclaimability;
pub mod transaction;

pub use codec::*;
pub use commit_log_entry::*;
pub use commit_log_facade::*;
pub use compaction::*;
pub use durability_fence::*;
pub use file::*;
pub use gc::*;
pub use gc_eligibility::*;
pub use manager::*;
pub use record::*;
pub use record_bounds::*;
pub use segment::*;
pub use segment_reclaimability::*;
pub use transaction::*;
