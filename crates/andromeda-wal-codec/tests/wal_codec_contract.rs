use andromeda_wal_codec::{
    WAL_RECORD_HEADER_LEN, WalCodecRecordFrame, WalScanStopReason, decode_frame_header,
    decode_wal_record_frame, encode_wal_record_frame, encoded_wal_record_frame_len,
    scan_wal_record_frames_from,
};

fn frame(lsn: u64, previous_lsn: Option<u64>, payload: impl Into<Vec<u8>>) -> WalCodecRecordFrame {
    WalCodecRecordFrame {
        kind_tag: 1,
        lsn,
        previous_lsn,
        transaction_id: Some(7),
        payload: payload.into(),
        record_checksum: 0xba99_06d0_e7da_b962,
    }
}

fn encoded(frames: &[WalCodecRecordFrame]) -> Vec<u8> {
    frames
        .iter()
        .flat_map(|frame| encode_wal_record_frame(frame).unwrap())
        .collect()
}

#[test]
fn canonical_golden_frame_bytes_are_stable() {
    let frame = frame(1, None, []);
    let encoded = encode_wal_record_frame(&frame).unwrap();

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
    assert_eq!(header.lsn, 1);
    assert_eq!(header.previous_lsn, None);
    assert_eq!(header.total_length, WAL_RECORD_HEADER_LEN as u64);
    assert_eq!(
        encoded_wal_record_frame_len(0).unwrap(),
        encoded.len() as u64
    );

    let (decoded, consumed) = decode_wal_record_frame(&encoded).unwrap().unwrap();
    assert_eq!(decoded, frame);
    assert_eq!(consumed, WAL_RECORD_HEADER_LEN);
}

#[test]
fn scan_accepts_valid_multi_frame_chain() {
    let frames = vec![frame(1, None, []), frame(2, Some(1), b"row".to_vec())];
    let buffer = encoded(&frames);

    let scan = scan_wal_record_frames_from(&buffer, 1, None);

    assert!(scan.is_complete());
    assert_eq!(scan.records, frames);
    assert_eq!(scan.valid_bytes, buffer.len());
    assert_eq!(scan.last_valid_lsn, Some(2));
}

#[test]
fn scan_reports_lsn_gap_after_last_valid_frame() {
    let frames = vec![frame(1, None, []), frame(3, Some(1), [])];
    let first_len = encode_wal_record_frame(&frames[0]).unwrap().len();

    let scan = scan_wal_record_frames_from(&encoded(&frames), 1, None);

    let stop = scan.stopped.unwrap();
    assert_eq!(stop.reason, WalScanStopReason::LsnGap);
    assert_eq!(stop.offset, first_len);
    assert_eq!(scan.valid_bytes, first_len);
    assert_eq!(scan.last_valid_lsn, Some(1));
}
