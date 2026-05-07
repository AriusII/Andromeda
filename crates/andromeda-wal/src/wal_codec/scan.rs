use crate::{Lsn, WalRecord};

use super::{WAL_RECORD_HEADER_LEN, frame::decode_frame_header, record::decode_wal_record_frame};

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
pub struct WalScanResult {
    pub records: Vec<WalRecord>,
    pub valid_bytes: usize,
    pub last_valid_lsn: Option<Lsn>,
    pub stopped: Option<WalScanStop>,
}

impl WalScanResult {
    pub fn is_complete(&self) -> bool {
        self.stopped.is_none()
    }
}

pub fn scan_wal_records(buffer: &[u8]) -> WalScanResult {
    scan_wal_records_from(buffer, Lsn::new(1), None)
}

pub fn scan_wal_records_from(
    buffer: &[u8],
    first_lsn: Lsn,
    base_previous_lsn: Option<Lsn>,
) -> WalScanResult {
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

        let header = match decode_frame_header(&buffer[offset..offset + WAL_RECORD_HEADER_LEN]) {
            Ok(header) => header,
            Err(_) => {
                return scan_result(
                    records,
                    offset,
                    last_valid_lsn,
                    WalScanStopReason::CorruptHeader,
                );
            }
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
            }
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

        let record = match decode_wal_record_frame(&buffer[offset..offset + total_length]) {
            Ok(Some((record, consumed))) if consumed == total_length => record,
            _ => {
                return scan_result(
                    records,
                    offset,
                    last_valid_lsn,
                    WalScanStopReason::CorruptRecord,
                );
            }
        };

        previous_lsn_for_chain = Some(record.header.lsn);
        last_valid_lsn = Some(record.header.lsn);
        next_lsn = record.header.lsn.checked_next();
        offset += total_length;
        records.push(record);
    }

    WalScanResult {
        records,
        valid_bytes: offset,
        last_valid_lsn,
        stopped: None,
    }
}

fn scan_result(
    records: Vec<WalRecord>,
    offset: usize,
    last_valid_lsn: Option<Lsn>,
    reason: WalScanStopReason,
) -> WalScanResult {
    WalScanResult {
        records,
        valid_bytes: offset,
        last_valid_lsn,
        stopped: Some(WalScanStop { offset, reason }),
    }
}
