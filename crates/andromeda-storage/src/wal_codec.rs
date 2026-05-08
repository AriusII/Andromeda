//! Legacy storage WAL codec compatibility surface.
//!
//! The typed WAL frame codec surface lives in `andromeda_wal::wal_codec`, backed
//! by the raw `andromeda_wal_codec` byte contract. Storage reexports it to
//! preserve existing crate-root codec paths while callers migrate.

pub use andromeda_wal::wal_codec::{
    WAL_BYTE_ORDER_LITTLE_ENDIAN, WAL_FORMAT_VERSION, WAL_FORMAT_VERSION_V1, WAL_RECORD_HEADER_LEN,
    WAL_RECORD_MAGIC, WalFrameHeader, WalScanResult, WalScanStop, WalScanStopReason,
    decode_frame_header, decode_wal_record_frame, encode_wal_record, scan_wal_records,
    scan_wal_records_from,
};
