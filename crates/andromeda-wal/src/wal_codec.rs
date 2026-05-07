//! Canonical WAL frame codec, scan engine, and binary constants.
//!
//! This module is the **single owner** of:
//!
//! * `WAL_*` byte format constants (magic, header length, byte order, format
//!   version),
//! * the `WalFrameHeader` decoder,
//! * `encode_wal_record` / `decode_wal_record_frame`, and
//! * the streaming `scan_wal_records` engine plus its `WalScanResult`,
//!   `WalScanStop`, and `WalScanStopReason` types.
//!
//! [`crate::write_ahead_log::codec`] re-exports these items as a domain-shaped
//! facade; it must never define equivalents itself. The byte format is
//! load-bearing for crash recovery and must not change without an explicit
//! `WAL_FORMAT_VERSION` bump.
mod binary;
mod checksum;
mod frame;
mod record;
mod scan;

use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub use frame::{WalFrameHeader, decode_frame_header};
pub use record::{decode_wal_record_frame, encode_wal_record};
pub use scan::{
    WalScanResult, WalScanStop, WalScanStopReason, scan_wal_records, scan_wal_records_from,
};

pub const WAL_FORMAT_VERSION_V1: u16 = 1;
pub const WAL_FORMAT_VERSION: u16 = WAL_FORMAT_VERSION_V1;
pub const WAL_BYTE_ORDER_LITTLE_ENDIAN: u16 = 0x0102;
pub const WAL_RECORD_MAGIC: u64 = 0x414e_4452_4f57_414c;
pub const WAL_RECORD_HEADER_LEN: usize = 72;

pub(super) fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
