use andromeda_core::TransactionId;
use andromeda_wal::{
    InMemoryWal, Lsn, MemoryWal, WAL_BYTE_ORDER_LITTLE_ENDIAN, WAL_FORMAT_VERSION,
    WAL_FORMAT_VERSION_V1, WAL_RECORD_HEADER_LEN, WAL_RECORD_MAGIC, WalFrameHeader, WalRecord,
    WalRecordHeader, WalRecordKind, WalScanResult, WalScanStop, WalScanStopReason, WalSegment,
    WalSegmentDescriptor, decode_frame_header, decode_wal_record_frame, encode_wal_record,
    scan_wal_records, scan_wal_records_from, wal_record_checksum,
};

#[test]
fn root_exports_cover_pure_wal_primitives_and_codec_roundtrip() {
    let transaction_id = TransactionId::new(7);
    let record = WalRecord::from_parts(
        WalRecordKind::RowInsert,
        Lsn::new(1),
        None,
        Some(transaction_id),
        b"row".to_vec(),
    )
    .unwrap();

    let header: WalRecordHeader = record.header;
    assert_eq!(header.kind, WalRecordKind::RowInsert);
    assert_eq!(record.payload(), b"row");
    assert_eq!(
        header.checksum,
        wal_record_checksum(
            header.kind,
            header.lsn,
            header.previous_lsn,
            header.transaction_id,
            record.payload(),
        )
    );

    assert_eq!(WAL_FORMAT_VERSION, WAL_FORMAT_VERSION_V1);
    assert_eq!(WAL_BYTE_ORDER_LITTLE_ENDIAN, 0x0102);
    assert_eq!(WAL_RECORD_MAGIC, 0x414e_4452_4f57_414c);

    let encoded = encode_wal_record(&record).unwrap();
    assert_eq!(
        encoded.len(),
        WAL_RECORD_HEADER_LEN + record.payload().len()
    );

    let frame_header: WalFrameHeader = decode_frame_header(&encoded).unwrap();
    assert_eq!(frame_header.lsn, Lsn::new(1));
    assert_eq!(frame_header.kind().unwrap(), WalRecordKind::RowInsert);

    let (decoded, consumed) = decode_wal_record_frame(&encoded).unwrap().unwrap();
    assert_eq!(decoded, record);
    assert_eq!(consumed, encoded.len());

    let scan: WalScanResult = scan_wal_records(&encoded);
    assert!(scan.is_complete());
    assert_eq!(scan.records, vec![record.clone()]);
    assert_eq!(scan.valid_bytes, encoded.len());
    assert_eq!(scan.last_valid_lsn, Some(Lsn::new(1)));

    let anchored_scan = scan_wal_records_from(&encoded, Lsn::new(1), None);
    assert!(anchored_scan.is_complete());

    let truncated = scan_wal_records(&encoded[..WAL_RECORD_HEADER_LEN - 1]);
    let stop: WalScanStop = truncated.stopped.unwrap();
    assert_eq!(stop.reason, WalScanStopReason::TruncatedHeader);
}

#[test]
fn nested_module_exports_match_root_export_identities() {
    let root_lsn: Lsn = andromeda_wal::lsn::Lsn::new(1);
    let nested_lsn: andromeda_wal::lsn::Lsn = root_lsn;
    assert_eq!(nested_lsn.get(), 1);

    let mut root_wal: InMemoryWal = andromeda_wal::write_ahead_log::InMemoryWal::new();
    let _: MemoryWal = andromeda_wal::write_ahead_log::MemoryWal::new();
    let transaction_id = TransactionId::new(11);
    assert_eq!(
        root_wal.append_tx_begin(transaction_id).unwrap(),
        Lsn::new(1)
    );
    assert_eq!(
        root_wal.append_tx_commit(transaction_id).unwrap(),
        Lsn::new(2)
    );

    let records = root_wal.records().to_vec();
    let descriptor: WalSegmentDescriptor =
        andromeda_wal::write_ahead_log::segment::WalSegmentDescriptor::for_records(
            1, None, &records,
        )
        .unwrap();
    let nested_descriptor: andromeda_wal::wal_segment::WalSegmentDescriptor = descriptor;
    let segment: WalSegment =
        andromeda_wal::write_ahead_log::segment::WalSegment::new(nested_descriptor, records)
            .unwrap();
    assert_eq!(segment.descriptor.record_count, 2);

    let encoded = andromeda_wal::wal_codec::encode_wal_record(&segment.records[0]).unwrap();
    let decoded_header: andromeda_wal::wal_codec::WalFrameHeader =
        andromeda_wal::write_ahead_log::codec::decode_frame_header(&encoded).unwrap();
    assert_eq!(decoded_header.kind().unwrap(), WalRecordKind::TxBegin);

    let scan: andromeda_wal::write_ahead_log::codec::WalScanResult =
        andromeda_wal::wal_codec::scan_wal_records_from(&encoded, Lsn::new(1), None);
    assert!(scan.is_complete());
    assert_eq!(scan.records.len(), 1);
}
