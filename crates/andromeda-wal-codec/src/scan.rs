use andromeda_error::AndromedaResult;

use crate::{
    frame::{WalCodecRecordFrame, decode_wal_record_frame},
    header::{WAL_RECORD_HEADER_LEN, WalCodecFrameHeader, decode_frame_header},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalScanStopReason {
    TruncatedHeader,
    TruncatedRecord,
    CorruptHeader,
    CorruptRecord,
    LsnGap,
    DuplicateOrReorderedLsn,
    PreviousLsnMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalScanStop {
    pub offset: usize,
    pub reason: WalScanStopReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalCodecScanResult<T> {
    pub records: Vec<T>,
    pub valid_bytes: usize,
    pub last_valid_lsn: Option<u64>,
    pub stopped: Option<WalScanStop>,
}

impl<T> WalCodecScanResult<T> {
    pub fn is_complete(&self) -> bool {
        self.stopped.is_none()
    }
}

pub fn scan_wal_record_frames(buffer: &[u8]) -> WalCodecScanResult<WalCodecRecordFrame> {
    scan_wal_record_frames_from(buffer, 1, None)
}

pub fn scan_wal_record_frames_from(
    buffer: &[u8],
    first_lsn: u64,
    base_previous_lsn: Option<u64>,
) -> WalCodecScanResult<WalCodecRecordFrame> {
    scan_wal_record_frames_from_with(
        buffer,
        first_lsn,
        base_previous_lsn,
        |_| Ok(()),
        decode_wal_record_frame,
    )
}

pub fn scan_wal_record_frames_from_with<T, ValidateHeader, DecodeFrame>(
    buffer: &[u8],
    first_lsn: u64,
    base_previous_lsn: Option<u64>,
    mut validate_header: ValidateHeader,
    mut decode_frame: DecodeFrame,
) -> WalCodecScanResult<T>
where
    ValidateHeader: FnMut(&WalCodecFrameHeader) -> AndromedaResult<()>,
    DecodeFrame: FnMut(&[u8]) -> AndromedaResult<Option<(T, usize)>>,
{
    let mut records = Vec::new();
    let mut offset = 0;
    let mut previous_lsn_for_chain = base_previous_lsn;
    let mut last_valid_lsn = None;
    let mut next_lsn = Some(first_lsn);

    while offset < buffer.len() {
        if buffer.len() - offset < WAL_RECORD_HEADER_LEN {
            return scan_result(
                records,
                offset,
                last_valid_lsn,
                WalScanStopReason::TruncatedHeader,
            );
        }

        let header = match decode_frame_header(&buffer[offset..offset + WAL_RECORD_HEADER_LEN])
            .and_then(|header| {
                validate_header(&header)?;
                Ok(header)
            }) {
            Ok(header) => header,
            Err(_) => {
                return scan_result(
                    records,
                    offset,
                    last_valid_lsn,
                    WalScanStopReason::CorruptHeader,
                );
            },
        };

        let total_length = match usize::try_from(header.total_length) {
            Ok(total_length) => total_length,
            Err(_) => {
                return scan_result(
                    records,
                    offset,
                    last_valid_lsn,
                    WalScanStopReason::CorruptHeader,
                );
            },
        };
        if buffer.len() - offset < total_length {
            return scan_result(
                records,
                offset,
                last_valid_lsn,
                WalScanStopReason::TruncatedRecord,
            );
        }

        let Some(expected_lsn) = next_lsn else {
            return scan_result(
                records,
                offset,
                last_valid_lsn,
                WalScanStopReason::DuplicateOrReorderedLsn,
            );
        };

        if header.lsn < expected_lsn {
            return scan_result(
                records,
                offset,
                last_valid_lsn,
                WalScanStopReason::DuplicateOrReorderedLsn,
            );
        }
        if header.lsn > expected_lsn {
            return scan_result(records, offset, last_valid_lsn, WalScanStopReason::LsnGap);
        }
        if previous_lsn_for_chain != header.previous_lsn {
            return scan_result(
                records,
                offset,
                last_valid_lsn,
                WalScanStopReason::PreviousLsnMismatch,
            );
        }

        let record = match decode_frame(&buffer[offset..offset + total_length]) {
            Ok(Some((record, consumed))) if consumed == total_length => record,
            _ => {
                return scan_result(
                    records,
                    offset,
                    last_valid_lsn,
                    WalScanStopReason::CorruptRecord,
                );
            },
        };

        previous_lsn_for_chain = Some(header.lsn);
        last_valid_lsn = Some(header.lsn);
        next_lsn = header.lsn.checked_add(1);
        offset += total_length;
        records.push(record);
    }

    WalCodecScanResult {
        records,
        valid_bytes: offset,
        last_valid_lsn,
        stopped: None,
    }
}

fn scan_result<T>(
    records: Vec<T>,
    offset: usize,
    last_valid_lsn: Option<u64>,
    reason: WalScanStopReason,
) -> WalCodecScanResult<T> {
    WalCodecScanResult {
        records,
        valid_bytes: offset,
        last_valid_lsn,
        stopped: Some(WalScanStop { offset, reason }),
    }
}
