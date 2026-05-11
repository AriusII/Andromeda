#![forbid(unsafe_code)]
#![doc = r#"
Explicit WAL frame codec for Andromeda.

This crate owns byte-format constants, little-endian field encoding, WAL frame
header validation, raw frame encode/decode, and the bounded scan loop. It does
not own WAL record semantics, replay policy, fsync behavior, or storage
recovery. Owner crates provide typed record validation through narrow wrapper
functions.
"#]

mod binary;
mod checksum;
mod frame;
mod header;
mod scan;

use andromeda_error::{AndromedaError, AndromedaErrorKind};

pub use checksum::fnv64_nonzero;
pub use frame::{
    WalCodecRecordFrame, decode_wal_record_frame, encode_wal_record_frame,
    encoded_wal_record_frame_len,
};
pub use header::{
    WAL_BYTE_ORDER_LITTLE_ENDIAN, WAL_FORMAT_VERSION, WAL_FORMAT_VERSION_V1, WAL_RECORD_HEADER_LEN,
    WAL_RECORD_MAGIC, WalCodecFrameHeader, decode_frame_header,
};
pub use scan::{
    WalCodecScanResult, WalScanStop, WalScanStopReason, scan_wal_record_frames,
    scan_wal_record_frames_from, scan_wal_record_frames_from_with,
};

pub(crate) fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
