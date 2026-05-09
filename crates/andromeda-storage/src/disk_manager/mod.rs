//! Compatibility facade for disk page-store ownership.
//!
//! Disk manager, file-backed page I/O, atomic writes, extent mapping, and
//! page-integrity contracts now live in `andromeda-disk-page-store`.

pub use andromeda_disk_page_store::{
    DiskManager, DiskManagerError, DiskPageStore, FileDiskManager, PageIntegrityMode,
};
