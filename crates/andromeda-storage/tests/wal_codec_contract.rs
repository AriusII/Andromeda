use andromeda_core::TransactionId;
use andromeda_storage::{
    encode_wal_record, scan_wal_records, Lsn, WalRecord, WalRecordKind, WalScanStopReason,
};

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
