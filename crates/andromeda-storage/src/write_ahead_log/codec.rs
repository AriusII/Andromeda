//! Compatibility reexports for the WAL frame codec.
//!
//! The typed codec moved to `andromeda_wal::write_ahead_log::codec`, backed by
//! `andromeda_wal_codec`. Storage keeps this module as a stable facade for
//! existing callers.

pub use andromeda_wal::write_ahead_log::codec::{
    WAL_BYTE_ORDER_LITTLE_ENDIAN, WAL_FORMAT_VERSION, WAL_FORMAT_VERSION_V1, WAL_RECORD_HEADER_LEN,
    WAL_RECORD_MAGIC, WalFrameHeader, WalScanResult, WalScanStop, WalScanStopReason,
    decode_frame_header, decode_wal_record_frame, encode_wal_record, scan_wal_records,
    scan_wal_records_from,
};
