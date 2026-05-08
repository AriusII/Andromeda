use andromeda_core::TransactionId;
use andromeda_wal::{
    Lsn, WAL_RECORD_HEADER_LEN, WalRecord, WalRecordKind, WalScanResult, WalScanStopReason,
    decode_frame_header, decode_wal_record_frame, encode_wal_record, encoded_wal_record_len,
    scan_wal_records_from,
};

fn record(
    kind: WalRecordKind,
    lsn: u64,
    previous_lsn: Option<u64>,
    payload: impl Into<Vec<u8>>,
) -> WalRecord {
    WalRecord::from_parts(
        kind,
        Lsn::new(lsn),
        previous_lsn.map(Lsn::new),
        Some(TransactionId::new(7)),
        payload,
    )
    .unwrap()
}

fn encoded(records: &[WalRecord]) -> Vec<u8> {
    records
        .iter()
        .flat_map(|record| encode_wal_record(record).unwrap())
        .collect()
}

fn scan(buffer: &[u8]) -> WalScanResult {
    scan_wal_records_from(buffer, Lsn::new(1), None)
}

fn assert_stop(
    scan: WalScanResult,
    reason: WalScanStopReason,
    offset: usize,
    valid_lsn: Option<u64>,
) {
    let stop = scan.stopped.unwrap();
    assert_eq!(stop.reason, reason);
    assert_eq!(stop.offset, offset);
    assert_eq!(scan.valid_bytes, offset);
    assert_eq!(scan.last_valid_lsn, valid_lsn.map(Lsn::new));
}

#[test]
fn canonical_golden_record_bytes_are_stable() {
    let record = record(WalRecordKind::TxBegin, 1, None, []);
    let encoded = encode_wal_record(&record).unwrap();

    assert_eq!(
        encoded,
        vec![
            0x4c, 0x41, 0x57, 0x4f, 0x52, 0x44, 0x4e, 0x41, 0x01, 0x00, 0x48, 0x00, 0x48, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x62, 0xb9, 0xda, 0xe7, 0xd0, 0x06, 0x99, 0xba, 0xe8, 0x9e, 0x23, 0xa5, 0xd9, 0x40,
            0x8e, 0xbe,
        ]
    );

    let header = decode_frame_header(&encoded).unwrap();
    assert_eq!(header.lsn, Lsn::new(1));
    assert_eq!(header.previous_lsn, None);
    assert_eq!(header.kind().unwrap(), WalRecordKind::TxBegin);

    let (decoded, consumed) = decode_wal_record_frame(&encoded).unwrap().unwrap();
    assert_eq!(decoded, record);
    assert_eq!(consumed, WAL_RECORD_HEADER_LEN);
}

#[test]
fn encoded_wal_record_len_matches_actual_encode() {
    let records = [
        record(WalRecordKind::TxBegin, 1, None, []),
        record(WalRecordKind::RowInsert, 2, Some(1), b"row".to_vec()),
        record(
            WalRecordKind::CatalogChangeApply,
            3,
            Some(2),
            vec![0xa5; 257],
        ),
    ];

    for record in records {
        let encoded = encode_wal_record(&record).unwrap();
        let frame_header = decode_frame_header(&encoded).unwrap();

        assert_eq!(
            encoded_wal_record_len(&record).unwrap(),
            encoded.len() as u64
        );
        assert_eq!(
            encoded_wal_record_len(&record).unwrap(),
            frame_header.total_length
        );
    }
}

#[test]
fn scan_accepts_valid_multi_record_chain() {
    let records = vec![
        record(WalRecordKind::TxBegin, 1, None, []),
        record(WalRecordKind::RowInsert, 2, Some(1), b"row".to_vec()),
        record(WalRecordKind::TxCommit, 3, Some(2), []),
    ];
    let buffer = encoded(&records);

    let scan = scan(&buffer);

    assert!(scan.is_complete());
    assert_eq!(scan.records, records);
    assert_eq!(scan.valid_bytes, buffer.len());
    assert_eq!(scan.last_valid_lsn, Some(Lsn::new(3)));
}

#[test]
fn scan_reports_truncated_header_without_valid_bytes() {
    let buffer = encode_wal_record(&record(WalRecordKind::TxBegin, 1, None, [])).unwrap();

    assert_stop(
        scan(&buffer[..WAL_RECORD_HEADER_LEN - 1]),
        WalScanStopReason::TruncatedHeader,
        0,
        None,
    );
}

#[test]
fn scan_reports_truncated_record_at_record_start() {
    let buffer =
        encode_wal_record(&record(WalRecordKind::RowInsert, 1, None, b"row".to_vec())).unwrap();

    assert_stop(
        scan(&buffer[..buffer.len() - 1]),
        WalScanStopReason::TruncatedRecord,
        0,
        None,
    );
}

#[test]
fn scan_reports_corrupt_header_at_record_start() {
    let mut buffer = encode_wal_record(&record(WalRecordKind::TxBegin, 1, None, [])).unwrap();
    buffer[0] ^= 0xff;

    assert_stop(scan(&buffer), WalScanStopReason::CorruptHeader, 0, None);
}

#[test]
fn scan_reports_corrupt_record_after_header_and_chain_validate() {
    let mut buffer =
        encode_wal_record(&record(WalRecordKind::RowInsert, 1, None, b"row".to_vec())).unwrap();
    buffer[WAL_RECORD_HEADER_LEN] ^= 0xff;

    assert_stop(scan(&buffer), WalScanStopReason::CorruptRecord, 0, None);
}

#[test]
fn scan_reports_lsn_gap_after_last_valid_record() {
    let records = vec![
        record(WalRecordKind::TxBegin, 1, None, []),
        record(WalRecordKind::TxCommit, 3, Some(1), []),
    ];
    let first_len = encode_wal_record(&records[0]).unwrap().len();

    assert_stop(
        scan(&encoded(&records)),
        WalScanStopReason::LsnGap,
        first_len,
        Some(1),
    );
}

#[test]
fn scan_reports_duplicate_or_reordered_lsn_after_last_valid_record() {
    let records = vec![
        record(WalRecordKind::TxBegin, 1, None, []),
        record(WalRecordKind::TxCommit, 1, None, []),
    ];
    let first_len = encode_wal_record(&records[0]).unwrap().len();

    assert_stop(
        scan(&encoded(&records)),
        WalScanStopReason::DuplicateOrReorderedLsn,
        first_len,
        Some(1),
    );
}

#[test]
fn scan_reports_previous_lsn_mismatch_after_last_valid_record() {
    let records = vec![
        record(WalRecordKind::TxBegin, 1, None, []),
        record(WalRecordKind::TxCommit, 2, None, []),
    ];
    let first_len = encode_wal_record(&records[0]).unwrap().len();

    assert_stop(
        scan(&encoded(&records)),
        WalScanStopReason::PreviousLsnMismatch,
        first_len,
        Some(1),
    );
}
