//! WAL frame codec and scan contracts.

pub use crate::{
    decode_frame_header, decode_wal_record_frame, encode_wal_record, scan_wal_records, scan_wal_records_from,
    WalFrameHeader, WalScanResult, WalScanStop, WalScanStopReason,
    WAL_FORMAT_VERSION, WAL_RECORD_HEADER_LEN, WAL_RECORD_MAGIC,
};
