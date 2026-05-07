//! Compatibility facade for the physical FileWal owner.

pub use crate::file_wal::{
    FILE_WAL_HEADER_LEN, FILE_WAL_MAGIC, FILE_WAL_MONO_SEGMENT_ID, FileWal, FileWalDiskScan,
    FileWalHeader, scan_file_wal,
};
