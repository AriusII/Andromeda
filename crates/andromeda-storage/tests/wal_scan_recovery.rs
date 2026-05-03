use andromeda_core::TransactionId;
use andromeda_storage::publication::DatabaseManifest;
use andromeda_storage::write_ahead_log::codec::{
    encode_wal_record, scan_wal_records, scan_wal_records_from, WalScanStopReason,
};
use andromeda_storage::write_ahead_log::record::{WalRecord, WalRecordKind};
use andromeda_storage::{
    DurableTransactionState, Lsn, RecoveryPlan, RedoRecordDecision, StartupMode,
};

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

fn record(
    kind: WalRecordKind,
    lsn: u64,
    previous_lsn: Option<u64>,
    transaction_id: Option<u64>,
    payload: &[u8],
) -> WalRecord {
    WalRecord::from_parts(
        kind,
        Lsn::new(lsn),
        previous_lsn.map(Lsn::new),
        transaction_id.map(TransactionId::new),
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
fn recovery_replays_only_committed_transaction_records() {
    let committed = 101;
    let rolled_back = 102;
    let incomplete = 103;
    let records = vec![
        record(WalRecordKind::TxBegin, 1, None, Some(committed), b""),
        record(
            WalRecordKind::RowInsert,
            2,
            Some(1),
            Some(committed),
            b"commit-row",
        ),
        record(WalRecordKind::TxCommit, 3, Some(2), Some(committed), b""),
        record(WalRecordKind::TxBegin, 4, Some(3), Some(rolled_back), b""),
        record(
            WalRecordKind::RowInsert,
            5,
            Some(4),
            Some(rolled_back),
            b"rollback-row",
        ),
        record(
            WalRecordKind::TxRollback,
            6,
            Some(5),
            Some(rolled_back),
            b"",
        ),
        record(WalRecordKind::TxBegin, 7, Some(6), Some(incomplete), b""),
        record(
            WalRecordKind::RowInsert,
            8,
            Some(7),
            Some(incomplete),
            b"incomplete-row",
        ),
    ];

    let plan = RecoveryPlan::from_manifest_and_wal(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &records,
    )
        .unwrap();

    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
    assert_eq!(
        plan.committed_replay_lsns().collect::<Vec<_>>(),
        vec![Lsn::new(2)]
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(2))
            .unwrap()
            .transaction_state,
        Some(DurableTransactionState::Committed)
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(5))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipRolledBackTransaction
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(8))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

#[test]
fn recovery_requires_manifest_start_to_be_covered_and_anchored() {
    let missing_start = vec![record(
        WalRecordKind::PageAllocate,
        6,
        Some(5),
        None,
        b"page",
    )];
    assert!(
        RecoveryPlan::from_manifest_and_wal(
            &manifest(Lsn::new(5)),
            StartupMode::SafeStart,
            &missing_start,
        )
            .is_err()
    );

    let unanchored_start = vec![record(WalRecordKind::PageAllocate, 5, None, None, b"page")];
    assert!(
        RecoveryPlan::from_manifest_and_wal(
            &manifest(Lsn::new(5)),
            StartupMode::SafeStart,
            &unanchored_start,
        )
            .is_err()
    );

    let anchored_start = vec![record(
        WalRecordKind::PageAllocate,
        5,
        Some(4),
        None,
        b"page",
    )];
    let plan = RecoveryPlan::from_manifest_and_wal(
        &manifest(Lsn::new(5)),
        StartupMode::SafeStart,
        &anchored_start,
    )
        .unwrap();
    assert_eq!(plan.coverage.first_replay_record_lsn, Some(Lsn::new(5)));
    assert_eq!(plan.coverage.record_count_in_redo_range, 1);
}

#[test]
fn recovery_from_scan_keeps_valid_prefix_and_records_tail_boundary() {
    let records = vec![
        record(WalRecordKind::TxBegin, 1, None, Some(201), b""),
        record(WalRecordKind::RowUpdate, 2, Some(1), Some(201), b"durable"),
        record(WalRecordKind::TxCommit, 3, Some(2), Some(201), b""),
        record(WalRecordKind::RowInsert, 4, Some(3), Some(202), b"tail"),
    ];
    let mut encoded = encode_records(&records);
    encoded.truncate(encoded.len() - 1);

    let scan = scan_wal_records(&encoded);
    assert_eq!(scan.records.len(), 3);
    assert_eq!(
        scan.stopped.unwrap().reason,
        WalScanStopReason::TruncatedRecord
    );

    let plan = RecoveryPlan::from_manifest_and_wal_scan(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &scan,
    )
        .unwrap();

    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
    assert_eq!(
        plan.wal_scan_stop().unwrap().reason,
        WalScanStopReason::TruncatedRecord
    );
}

#[test]
fn recovery_from_scan_rejects_non_recoverable_chain_boundaries() {
    let records = vec![tx_record(1, None, b"first"), tx_record(3, Some(1), b"gap")];
    let scan = scan_wal_records(&encode_records(&records));

    assert_eq!(scan.stopped.unwrap().reason, WalScanStopReason::LsnGap);
    assert!(
        RecoveryPlan::from_manifest_and_wal_scan(
            &manifest(Lsn::new(1)),
            StartupMode::SafeStart,
            &scan,
        )
            .is_err()
    );
}

#[test]
fn wal_scan_from_manifest_boundary_accepts_expected_previous_lsn() {
    let records = vec![
        record(WalRecordKind::PageAllocate, 5, Some(4), None, b"page"),
        record(WalRecordKind::PageFormat, 6, Some(5), None, b"format"),
    ];

    let scan = scan_wal_records_from(&encode_records(&records), Lsn::new(5), Some(Lsn::new(4)));

    assert!(scan.is_complete());
    assert_eq!(scan.records, records);
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
