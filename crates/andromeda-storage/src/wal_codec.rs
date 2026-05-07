//! Legacy storage WAL codec compatibility surface.
//!
//! The canonical WAL frame codec and byte-format constants now live in
//! `andromeda_wal::wal_codec`. Storage reexports them to preserve existing
//! crate-root codec paths while the workspace migrates callers.

pub use andromeda_wal::wal_codec::{
    WAL_BYTE_ORDER_LITTLE_ENDIAN, WAL_FORMAT_VERSION, WAL_FORMAT_VERSION_V1, WAL_RECORD_HEADER_LEN,
    WAL_RECORD_MAGIC, WalFrameHeader, WalScanResult, WalScanStop, WalScanStopReason,
    decode_frame_header, decode_wal_record_frame, encode_wal_record, scan_wal_records,
    scan_wal_records_from,
};
