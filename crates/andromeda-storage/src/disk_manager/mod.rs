//! Real disk I/O backing for buffer pool page flushes and reads.
//!
//! Invariants:
//! - Deterministic extent-to-file mapping.
//! - WAL-before-page remains enforced by higher layers; `durable_lsn` is accepted but
//!   not interpreted here.
//! - Page-integrity checks are explicit about current capability. Header CRC stamping
//!   is disabled until page-header fields are persisted by the page codec.

mod atomic_write;
mod error;
mod extent_map;
mod file;
mod interface;
mod integrity;
mod page_store;

pub use error::DiskManagerError;
pub use file::FileDiskManager;
pub use interface::DiskManager;
pub use integrity::PageIntegrityMode;
pub use page_store::DiskPageStore;
