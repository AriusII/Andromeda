//! Write-ahead log domain facade.
//!
//! The WAL record model, byte codec, and segment descriptor stay available from
//! the crate root for compatibility. This module provides a coherent domain
//! hierarchy for new code and external integration tests.

pub mod codec;
pub mod file {
    pub use crate::{
        FILE_WAL_HEADER_LEN, FILE_WAL_MAGIC, FILE_WAL_MONO_SEGMENT_ID, FileWal, FileWalDiskScan,
        FileWalHeader, recover_from_file_wal, scan_file_wal,
    };
}
pub mod record;
pub mod segment;

pub use codec::*;
pub use file::*;
pub use record::*;
pub use segment::*;
