//! Real disk I/O backing for buffer pool page flushes and reads.
//!
//! Invariants:
//! - Deterministic extent-to-file mapping.
//! - Raw `FileDiskManager` performs fixed-offset page I/O; `DiskPageStore` is the
//!   buffer-pool façade that enforces WAL-before-page flush ordering.
//! - Page-integrity checks are opt-in per manager and validate persisted layout bytes
//!   when `PageIntegrityMode::HeaderCrc32` is enabled.

mod atomic_write;
mod error;
mod extent_map;
mod file;
mod integrity;
mod interface;
mod page_store;

pub use error::DiskManagerError;
pub use file::FileDiskManager;
pub use integrity::PageIntegrityMode;
pub use interface::DiskManager;
pub use page_store::DiskPageStore;
