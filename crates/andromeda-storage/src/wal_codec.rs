mod binary;
mod checksum;
mod frame;
mod record;
mod scan;

use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub use frame::{decode_frame_header, WalFrameHeader};
pub use record::{decode_wal_record_frame, encode_wal_record};
pub use scan::{
    scan_wal_records, scan_wal_records_from, WalScanResult, WalScanStop, WalScanStopReason,
};

pub const WAL_FORMAT_VERSION_V1: u16 = 1;
pub const WAL_FORMAT_VERSION: u16 = WAL_FORMAT_VERSION_V1;
pub const WAL_BYTE_ORDER_LITTLE_ENDIAN: u16 = 0x0102;
pub const WAL_RECORD_MAGIC: u64 = 0x414e_4452_4f57_414c;
pub const WAL_RECORD_HEADER_LEN: usize = 72;

pub(super) fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Lsn, WalRecord, WalRecordKind};
    use andromeda_core::TransactionId;

    fn tx_record(lsn: u64, previous_lsn: Option<u64>, payload: &[u8]) -> WalRecord {
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(lsn),
            previous_lsn.map(Lsn::new),
            Some(TransactionId::new(7)),
            payload,
        )
        .unwrap()
    }

    fn encode_records(records: &[WalRecord]) -> Vec<u8> {
        let mut encoded = Vec::new();
        for record in records {
            encoded.extend(encode_wal_record(record).unwrap());
        }
        encoded
    }

    #[test]
    fn valid_record_scan_returns_all_records() {
        let records = vec![tx_record(1, None, b"a"), tx_record(2, Some(1), b"bb")];
        let scan = scan_wal_records(&encode_records(&records));

        assert!(scan.is_complete());
        assert_eq!(scan.records, records);
        assert_eq!(scan.last_valid_lsn, Some(Lsn::new(2)));
    }

    #[test]
    fn scan_stops_at_truncated_tail_and_keeps_prefix() {
        let records = vec![tx_record(1, None, b"a"), tx_record(2, Some(1), b"bb")];
        let mut encoded = encode_records(&records);
        encoded.truncate(encoded.len() - 1);

        let scan = scan_wal_records(&encoded);

        assert_eq!(scan.records, vec![records[0].clone()]);
        assert_eq!(
            scan.stopped.unwrap().reason,
            WalScanStopReason::TruncatedRecord
        );
    }

    #[test]
    fn scan_stops_at_corrupted_tail_and_keeps_prefix() {
        let records = vec![tx_record(1, None, b"a"), tx_record(2, Some(1), b"bb")];
        let mut encoded = encode_records(&records);
        let tail_payload_byte = encoded.len() - 1;
        encoded[tail_payload_byte] ^= 0x55;

        let scan = scan_wal_records(&encoded);

        assert_eq!(scan.records, vec![records[0].clone()]);
        assert_eq!(
            scan.stopped.unwrap().reason,
            WalScanStopReason::CorruptRecord
        );
    }

    #[test]
    fn scan_rejects_skipped_lsn() {
        let records = vec![tx_record(1, None, b"a"), tx_record(3, Some(1), b"bb")];
        let scan = scan_wal_records(&encode_records(&records));

        assert_eq!(scan.records.len(), 1);
        assert_eq!(scan.stopped.unwrap().reason, WalScanStopReason::LsnGap);
    }

    #[test]
    fn scan_rejects_duplicate_lsn() {
        let records = vec![tx_record(1, None, b"a"), tx_record(1, None, b"bb")];
        let scan = scan_wal_records(&encode_records(&records));

        assert_eq!(scan.records.len(), 1);
        assert_eq!(
            scan.stopped.unwrap().reason,
            WalScanStopReason::DuplicateOrReorderedLsn
        );
    }

    #[test]
    fn scan_rejects_previous_lsn_mismatch() {
        let records = vec![tx_record(1, None, b"a"), tx_record(2, None, b"bb")];
        let scan = scan_wal_records(&encode_records(&records));

        assert_eq!(scan.records.len(), 1);
        assert_eq!(
            scan.stopped.unwrap().reason,
            WalScanStopReason::PreviousLsnMismatch
        );
    }
}
