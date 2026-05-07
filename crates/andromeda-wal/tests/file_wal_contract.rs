use andromeda_core::{AndromedaErrorKind, TransactionId};
use andromeda_wal::{
    FILE_WAL_HEADER_LEN, FileWal, FileWalHeader, Lsn, WalRecord, WalRecordKind, WalScanStopReason,
    encode_wal_record, scan_file_wal,
};
use std::fs::{File, OpenOptions, metadata, remove_file};
use std::io::Write;
use std::path::{Path, PathBuf};

fn test_wal_path(test_name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "andromeda-wal-{test_name}-{}.wal",
        std::process::id()
    ))
}

fn encode_records(records: &[WalRecord]) -> Vec<u8> {
    let mut encoded = Vec::new();
    for record in records {
        encoded.extend(encode_wal_record(record).unwrap());
    }
    encoded
}

fn write_raw_wal_file(path: &Path, records: &[WalRecord]) {
    remove_file(path).ok();
    let encoded = encode_records(records);
    let durable_lsn = records
        .last()
        .map(|record| record.header.lsn)
        .unwrap_or(Lsn::ZERO);
    let header = FileWalHeader::new(durable_lsn, encoded.len() as u64, records.len() as u64);
    let mut file = File::create(path).unwrap();
    write_file_wal_header_for_test(&mut file, &header);
    file.write_all(&encoded).unwrap();
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
fn file_wal_open_initializes_empty_header_and_reopens() {
    let path = test_wal_path("open-empty");
    remove_file(&path).ok();

    {
        let wal = FileWal::open(&path).unwrap();
        assert!(wal.is_empty());
        assert_eq!(wal.durable_lsn(), Lsn::ZERO);
        assert_eq!(wal.append_bytes(), 0);
        assert_eq!(metadata(&path).unwrap().len(), FILE_WAL_HEADER_LEN as u64);
    }

    let scan = scan_file_wal(&path).unwrap();
    assert_eq!(scan.header.durable_lsn, Lsn::ZERO);
    assert_eq!(scan.physical_wal_bytes, 0);
    assert!(scan.scan.is_complete());

    remove_file(&path).ok();
}

#[test]
fn append_flush_and_reopen_preserves_strict_lsn_chain() {
    let path = test_wal_path("append-flush-reopen");
    remove_file(&path).ok();
    let tx = TransactionId::new(10);

    {
        let mut wal = FileWal::open(&path).unwrap();
        assert_eq!(wal.append_tx_begin(tx).unwrap(), Lsn::new(1));
        assert_eq!(
            wal.append_payload(WalRecordKind::RowInsert, Some(tx), b"row")
                .unwrap(),
            Lsn::new(2)
        );
        assert_eq!(wal.append_tx_commit(tx).unwrap(), Lsn::new(3));
        assert_eq!(wal.flush_all().unwrap(), Lsn::new(3));
    }

    let wal = FileWal::open(&path).unwrap();
    assert_eq!(wal.len(), 3);
    assert_eq!(wal.durable_lsn(), Lsn::new(3));
    assert_eq!(wal.records()[0].header.previous_lsn, None);
    assert_eq!(wal.records()[1].header.previous_lsn, Some(Lsn::new(1)));
    assert_eq!(wal.records()[2].header.previous_lsn, Some(Lsn::new(2)));

    remove_file(&path).ok();
}

#[test]
fn flush_through_updates_durable_header_prefix() {
    let path = test_wal_path("flush-through");
    remove_file(&path).ok();
    let tx = TransactionId::new(11);

    {
        let mut wal = FileWal::open(&path).unwrap();
        wal.append_tx_begin(tx).unwrap();
        let row_lsn = wal
            .append_payload(WalRecordKind::RowInsert, Some(tx), b"row")
            .unwrap();
        wal.append_tx_commit(tx).unwrap();
        assert_eq!(wal.flush_through(row_lsn).unwrap(), row_lsn);
        assert_eq!(wal.durable_lsn(), Lsn::new(2));
    }

    let scan = scan_file_wal(&path).unwrap();
    assert_eq!(scan.header.durable_lsn, Lsn::new(2));
    assert_eq!(scan.header.durable_record_count, 2);
    assert_eq!(scan.durable_lsn, Lsn::new(2));
    assert_eq!(scan.scan.records.len(), 2);

    remove_file(&path).ok();
}

#[test]
fn reopen_truncates_unflushed_physical_tail() {
    let path = test_wal_path("truncate-unflushed-tail");
    remove_file(&path).ok();
    let committed_tx = TransactionId::new(12);
    let unflushed_tx = TransactionId::new(13);

    {
        let mut wal = FileWal::open(&path).unwrap();
        wal.append_tx_begin(committed_tx).unwrap();
        wal.append_payload(WalRecordKind::RowInsert, Some(committed_tx), b"complete")
            .unwrap();
        wal.append_tx_commit(committed_tx).unwrap();
        wal.flush_all().unwrap();
        wal.append_tx_begin(unflushed_tx).unwrap();
        wal.append_payload(WalRecordKind::RowUpdate, Some(unflushed_tx), b"tail")
            .unwrap();
        assert_eq!(wal.last_lsn(), Some(Lsn::new(5)));
        assert_eq!(wal.durable_lsn(), Lsn::new(3));
    }

    let wal = FileWal::open(&path).unwrap();
    assert_eq!(wal.last_lsn(), Some(Lsn::new(3)));
    assert_eq!(wal.durable_lsn(), Lsn::new(3));
    assert_eq!(
        metadata(&path).unwrap().len(),
        FILE_WAL_HEADER_LEN as u64 + wal.durable_bytes()
    );

    remove_file(&path).ok();
}

#[test]
fn scan_reports_recoverable_tail_with_valid_prefix() {
    let path = test_wal_path("recoverable-tail");
    remove_file(&path).ok();
    let tx = TransactionId::new(14);

    {
        let mut wal = FileWal::open(&path).unwrap();
        wal.append_tx_begin(tx).unwrap();
        wal.append_payload(WalRecordKind::RowInsert, Some(tx), b"complete")
            .unwrap();
        wal.append_tx_commit(tx).unwrap();
        wal.append_tx_begin(TransactionId::new(15)).unwrap();
        wal.flush_all().unwrap();
    }

    let original_len = metadata(&path).unwrap().len();
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(original_len - 1)
        .unwrap();

    let scan = scan_file_wal(&path).unwrap();
    assert_eq!(
        scan.scan.stopped.unwrap().reason,
        WalScanStopReason::TruncatedHeader
    );
    assert_eq!(scan.scan.last_valid_lsn, Some(Lsn::new(3)));
    assert_eq!(scan.durable_lsn, Lsn::new(3));
    assert_eq!(scan.scan.records.len(), 3);

    remove_file(&path).ok();
}

#[test]
fn forensic_chain_break_scan_is_visible_and_open_rejects() {
    let path = test_wal_path("forensic-chain-break");
    let tx = TransactionId::new(16);
    let records = vec![
        WalRecord::from_parts(WalRecordKind::TxBegin, Lsn::new(1), None, Some(tx), b"").unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(2),
            None,
            Some(tx),
            b"bad-prev",
        )
        .unwrap(),
    ];
    write_raw_wal_file(&path, &records);

    let scan = scan_file_wal(&path).unwrap();
    assert_eq!(
        scan.scan.stopped.unwrap().reason,
        WalScanStopReason::PreviousLsnMismatch
    );
    assert_eq!(scan.scan.last_valid_lsn, Some(Lsn::new(1)));

    let error = FileWal::open(&path).unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);

    remove_file(&path).ok();
}
