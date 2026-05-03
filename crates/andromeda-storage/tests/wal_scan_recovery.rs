use andromeda_core::TransactionId;
use andromeda_storage::publication::DatabaseManifest;
use andromeda_storage::write_ahead_log::codec::{
    WalScanStopReason, encode_wal_record, scan_wal_records,
};
use andromeda_storage::write_ahead_log::record::{WalRecord, WalRecordKind};
use andromeda_storage::{Lsn, RecoveryPlan, StartupMode};

fn tx_record(lsn: u64, previous_lsn: Option<u64>, payload: &[u8]) -> WalRecord {
    WalRecord::from_parts(
        WalRecordKind::RowInsert,
        Lsn::new(lsn),
        previous_lsn.map(Lsn::new),
        Some(TransactionId::new(42)),
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

fn manifest(required_wal_start_lsn: Lsn) -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: required_wal_start_lsn,
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 7,
    }
}

#[test]
fn wal_scan_keeps_valid_prefix_for_truncated_and_corrupt_tail() {
    let records = vec![
        tx_record(1, None, b"first"),
        tx_record(2, Some(1), b"second"),
    ];
    let encoded = encode_records(&records);

    let mut truncated = encoded.clone();
    truncated.truncate(truncated.len() - 1);
    let truncated_scan = scan_wal_records(&truncated);
    assert_eq!(truncated_scan.records, vec![records[0].clone()]);
    assert_eq!(
        truncated_scan.valid_bytes,
        encode_records(&records[..1]).len()
    );
    assert_eq!(
        truncated_scan.stopped.unwrap().reason,
        WalScanStopReason::TruncatedRecord
    );

    let mut corrupt = encoded;
    let last_payload_byte = corrupt.len() - 1;
    corrupt[last_payload_byte] ^= 0x5a;
    let corrupt_scan = scan_wal_records(&corrupt);
    assert_eq!(corrupt_scan.records, vec![records[0].clone()]);
    assert_eq!(
        corrupt_scan.stopped.unwrap().reason,
        WalScanStopReason::CorruptRecord
    );
}

#[test]
fn wal_scan_and_recovery_reject_lsn_gap() {
    let records = vec![tx_record(1, None, b"first"), tx_record(3, Some(1), b"gap")];

    let scan = scan_wal_records(&encode_records(&records));
    assert_eq!(scan.records, vec![records[0].clone()]);
    assert_eq!(scan.stopped.unwrap().reason, WalScanStopReason::LsnGap);

    let recovery = RecoveryPlan::from_manifest_and_wal(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &records,
    );
    assert!(recovery.is_err());
}

#[test]
fn wal_scan_rejects_previous_lsn_chain_mismatch() {
    let records = vec![
        tx_record(1, None, b"first"),
        tx_record(2, None, b"wrong-previous"),
    ];

    let scan = scan_wal_records(&encode_records(&records));

    assert_eq!(scan.records, vec![records[0].clone()]);
    assert_eq!(
        scan.stopped.unwrap().reason,
        WalScanStopReason::PreviousLsnMismatch
    );
}
