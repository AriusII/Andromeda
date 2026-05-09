//! Re-export surface for the canonical WAL frame codec defined in
//! `crate::wal_codec`. Do not define encode/decode/scan items here.

pub use crate::{
    WAL_BYTE_ORDER_LITTLE_ENDIAN, WAL_FORMAT_VERSION, WAL_FORMAT_VERSION_V1, WAL_RECORD_HEADER_LEN,
    WAL_RECORD_MAGIC, WalFrameHeader, WalScanResult, WalScanStop, WalScanStopReason,
    decode_frame_header, decode_wal_record_frame, encode_wal_record, encoded_wal_record_len,
    scan_wal_records, scan_wal_records_from,
};
