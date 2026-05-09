use andromeda_error::AndromedaErrorKind;
use andromeda_types::TransactionId;
use andromeda_wal::{
    FILE_WAL_HEADER_LEN, FileWal, FileWalHeader, Lsn, WAL_RECORD_HEADER_LEN, WalRecord,
    WalRecordKind, WalScanStopReason, classify_durable_transactions, encode_wal_record,
    scan_file_wal,
};
use std::fs::{File, metadata, remove_file};
use std::io::Write;
use std::path::{Path, PathBuf};

fn test_wal_path(test_name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "andromeda-wal-recovery-{test_name}-{}.wal",
        std::process::id()
    ))
}

fn tx_record(
    kind: WalRecordKind,
    lsn: u64,
    previous_lsn: Option<u64>,
    transaction_id: TransactionId,
    payload: impl Into<Vec<u8>>,
) -> WalRecord {
    WalRecord::from_parts(
        kind,
        Lsn::new(lsn),
        previous_lsn.map(Lsn::new),
        Some(transaction_id),
        payload,
    )
    .unwrap()
}

fn encoded_records(records: &[WalRecord]) -> Vec<u8> {
    let mut encoded = Vec::new();
    for record in records {
        encoded.extend(encode_wal_record(record).unwrap());
    }
    encoded
}

fn encoded_record_len(record: &WalRecord) -> usize {
    encode_wal_record(record).unwrap().len()
}

fn write_raw_wal_file(path: &Path, durable_lsn: Lsn, durable_record_count: u64, bytes: &[u8]) {
    remove_file(path).ok();
    let header = FileWalHeader::new(durable_lsn, bytes.len() as u64, durable_record_count);
    let mut file = File::create(path).unwrap();
    write_file_wal_header_for_test(&mut file, &header);
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}

fn write_file_wal_header_for_test(file: &mut File, header: &FileWalHeader) {
    header.validate().unwrap();
    let mut bytes = [0; FILE_WAL_HEADER_LEN];
    write_u64(&mut bytes, 0, header.magic);
    write_u16(&mut bytes, 8, header.format_version);
    write_u16(&mut bytes, 10, header.byte_order);
    write_u32(&mut bytes, 12, header.header_length);
    write_u64(&mut bytes, 16, header.segment_id);
    write_u64(&mut bytes, 24, header.first_lsn.get());
    write_u64(&mut bytes, 32, header.base_previous_lsn.map_or(0, Lsn::get));
    write_u64(&mut bytes, 40, header.durable_lsn.get());
    write_u64(&mut bytes, 48, header.durable_bytes);
    write_u64(&mut bytes, 56, header.durable_record_count);
    write_u64(&mut bytes, 64, header.header_checksum);
    write_u64(&mut bytes, 72, header.reserved);
    file.write_all(&bytes).unwrap();
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn checksum_mismatch_stops_recovery_at_last_valid_record() {
    let path = test_wal_path("checksum-mismatch");
    let tx = TransactionId::new(31);
    let records = vec![
        tx_record(WalRecordKind::TxBegin, 1, None, tx, []),
        tx_record(WalRecordKind::RowInsert, 2, Some(1), tx, b"row-31".to_vec()),
        tx_record(WalRecordKind::TxCommit, 3, Some(2), tx, []),
    ];
    let first_len = encoded_record_len(&records[0]);
    let mut bytes = encoded_records(&records);
    bytes[first_len + WAL_RECORD_HEADER_LEN] ^= 0xff;
    write_raw_wal_file(&path, Lsn::new(3), records.len() as u64, &bytes);

    let scan = scan_file_wal(&path).unwrap();
    let stop = scan.scan.stopped.unwrap();
    assert_eq!(stop.reason, WalScanStopReason::CorruptRecord);
    assert_eq!(stop.offset, first_len);
    assert_eq!(scan.scan.valid_bytes, first_len);
    assert_eq!(scan.scan.last_valid_lsn, Some(Lsn::new(1)));
    assert_eq!(scan.durable_lsn, Lsn::new(1));

    let wal = FileWal::open(&path).unwrap();
    assert_eq!(
        wal.scan_stop().unwrap().reason,
        WalScanStopReason::CorruptRecord
    );
    assert_eq!(wal.durable_lsn(), Lsn::new(1));
    assert_eq!(wal.replay_durable(), vec![records[0].clone()]);
    assert_eq!(
        metadata(&path).unwrap().len(),
        FILE_WAL_HEADER_LEN as u64 + first_len as u64
    );

    remove_file(&path).ok();
}

#[test]
fn truncated_record_stops_recovery_at_last_complete_record() {
    let path = test_wal_path("truncated-record");
    let tx = TransactionId::new(32);
    let records = vec![
        tx_record(WalRecordKind::TxBegin, 1, None, tx, []),
        tx_record(WalRecordKind::RowInsert, 2, Some(1), tx, b"row-32".to_vec()),
        tx_record(
            WalRecordKind::RowUpdate,
            3,
            Some(2),
            tx,
            b"truncated-payload".to_vec(),
        ),
    ];
    let valid_prefix_len = encoded_record_len(&records[0]) + encoded_record_len(&records[1]);
    let mut bytes = encoded_records(&records);
    bytes.pop().unwrap();
    write_raw_wal_file(&path, Lsn::new(3), records.len() as u64, &bytes);

    let scan = scan_file_wal(&path).unwrap();
    let stop = scan.scan.stopped.unwrap();
    assert_eq!(stop.reason, WalScanStopReason::TruncatedRecord);
    assert_eq!(stop.offset, valid_prefix_len);
    assert_eq!(scan.scan.valid_bytes, valid_prefix_len);
    assert_eq!(scan.scan.last_valid_lsn, Some(Lsn::new(2)));
    assert_eq!(scan.durable_lsn, Lsn::new(2));

    let wal = FileWal::open(&path).unwrap();
    assert_eq!(
        wal.scan_stop().unwrap().reason,
        WalScanStopReason::TruncatedRecord
    );
    assert_eq!(wal.durable_lsn(), Lsn::new(2));
    assert_eq!(wal.replay_durable(), records[..2].to_vec());
    assert_eq!(
        metadata(&path).unwrap().len(),
        FILE_WAL_HEADER_LEN as u64 + valid_prefix_len as u64
    );

    remove_file(&path).ok();
}

#[test]
fn prev_lsn_chain_break_is_forensic_and_open_rejects() {
    let path = test_wal_path("prev-lsn-break");
    let tx = TransactionId::new(33);
    let records = vec![
        tx_record(WalRecordKind::TxBegin, 1, None, tx, []),
        tx_record(WalRecordKind::RowInsert, 2, None, tx, b"bad-chain".to_vec()),
    ];
    let bytes = encoded_records(&records);
    write_raw_wal_file(&path, Lsn::new(2), records.len() as u64, &bytes);

    let scan = scan_file_wal(&path).unwrap();
    let stop = scan.scan.stopped.unwrap();
    assert_eq!(stop.reason, WalScanStopReason::PreviousLsnMismatch);
    assert_eq!(stop.offset, encoded_record_len(&records[0]));
    assert_eq!(scan.scan.last_valid_lsn, Some(Lsn::new(1)));
    assert_eq!(scan.durable_lsn, Lsn::new(1));

    let error = FileWal::open(&path).unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);

    remove_file(&path).ok();
}

#[test]
fn replay_durable_stops_before_commit_that_was_not_flushed() {
    let path = test_wal_path("commit-not-flushed");
    remove_file(&path).ok();
    let tx = TransactionId::new(34);

    {
        let mut wal = FileWal::open(&path).unwrap();
        wal.append_tx_begin(tx).unwrap();
        let row_lsn = wal
            .append_payload(WalRecordKind::RowInsert, Some(tx), b"pending-row")
            .unwrap();
        let commit_lsn = wal.append_tx_commit(tx).unwrap();
        assert_eq!(row_lsn, Lsn::new(2));
        assert_eq!(commit_lsn, Lsn::new(3));
        assert_eq!(wal.flush_through(row_lsn).unwrap(), row_lsn);
    }

    let wal = FileWal::open(&path).unwrap();
    let replay = wal.replay_durable();

    assert_eq!(wal.durable_lsn(), Lsn::new(2));
    assert_eq!(wal.last_lsn(), Some(Lsn::new(2)));
    assert_eq!(
        replay
            .iter()
            .map(|record| record.header.kind)
            .collect::<Vec<_>>(),
        vec![WalRecordKind::TxBegin, WalRecordKind::RowInsert]
    );

    let classifications = classify_durable_transactions(&replay);
    assert!(classifications.committed.is_empty());
    assert_eq!(
        classifications
            .incomplete_transaction_ids()
            .collect::<Vec<_>>(),
        vec![tx]
    );

    remove_file(&path).ok();
}
