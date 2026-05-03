//! WAL frame codec and scan contracts.

pub use crate::{
    WAL_FORMAT_VERSION, WAL_RECORD_HEADER_LEN, WAL_RECORD_MAGIC, WalFrameHeader, WalScanResult,
    WalScanStop, WalScanStopReason, decode_frame_header, decode_wal_record_frame,
    encode_wal_record, scan_wal_records, scan_wal_records_from,
};
