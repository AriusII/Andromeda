use crate::support::{encode_records, manifest, record, tx_record};
use andromeda_observe::{
    CriticalDecisionKind, EventCorrelation, EventEnvelope, EventId, RecoveryTrace, TraceEvent,
    TraceId,
};
use andromeda_storage::write_ahead_log::codec::{
    WalScanStopReason, scan_wal_records, scan_wal_records_from,
};
use andromeda_storage::write_ahead_log::record::WalRecordKind;
use andromeda_storage::{Lsn, RecoveryPlan, StartupMode};

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
fn recovery_trace_marks_recoverable_wal_tail_stop_as_boundary_evidence() {
    let records = vec![
        record(WalRecordKind::TxBegin, 1, None, Some(211), b""),
        record(WalRecordKind::RowUpdate, 2, Some(1), Some(211), b"durable"),
        record(WalRecordKind::TxCommit, 3, Some(2), Some(211), b""),
        record(WalRecordKind::RowInsert, 4, Some(3), Some(212), b"tail"),
    ];
    let mut encoded = encode_records(&records);
    encoded.truncate(encoded.len() - 1);
    let scan = scan_wal_records(&encoded);

    let plan = RecoveryPlan::from_manifest_and_wal_scan(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &scan,
    )
    .expect("recoverable WAL tail truncation should retain durable prefix for recovery");
    let trace_projection = plan.trace_projection(TraceId::new(601));
    let trace = RecoveryTrace {
        trace_id: trace_projection.trace_id,
        last_durable_lsn: trace_projection.last_durable_lsn,
        corruption_boundary_lsn: trace_projection.corruption_boundary_lsn,
    };

    assert!(trace.proves_recovery_boundary());
    assert_eq!(trace.last_durable_lsn, plan.durable_lsn.get());
    assert_eq!(trace.corruption_boundary_lsn, Some(plan.durable_lsn.get()));

    let envelope = EventEnvelope::new(
        EventId::new(602),
        EventCorrelation {
            durable_lsn: Some(plan.durable_lsn.get()),
            ..EventCorrelation::empty()
        },
        TraceEvent::RecoveryStartup(trace),
    )
    .expect("recovery startup trace must carry durable WAL boundary evidence");

    assert_eq!(envelope.event.kind(), CriticalDecisionKind::RecoveryStartup);
    assert_eq!(
        envelope.correlation.durable_lsn,
        Some(plan.durable_lsn.get())
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
