use crate::support::{encode_records, recovery_manifest};
use andromeda_manifest::DatabaseManifest;
use andromeda_observability::{EventCorrelation, EventId, TraceId};
use andromeda_observe::{EventEnvelope, RecoveryTrace as ObserveRecoveryTrace, TraceEvent};
use andromeda_recovery::{RecoveryPlan, RedoRecordDecision, StartupMode};
use andromeda_types::TransactionId;
use andromeda_wal::{
    InMemoryWal, Lsn, WalRecord, WalRecordKind, WalScanStopReason, scan_wal_records,
};

#[test]
fn redo_plan_from_truncated_wal_scan_replays_only_complete_prefix_transactions() {
    let committed_tx = TransactionId::new(25);
    let tail_tx = TransactionId::new(26);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(committed_tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(committed_tx),
            b"complete".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(committed_tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(4),
            Some(Lsn::new(3)),
            Some(tail_tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowUpdate,
            Lsn::new(5),
            Some(Lsn::new(4)),
            Some(tail_tx),
            b"tail".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(6),
            Some(Lsn::new(5)),
            Some(tail_tx),
            Vec::new(),
        )
        .unwrap(),
    ];
    let mut encoded = encode_records(&records);
    encoded.truncate(encoded.len() - 1);
    let scan = scan_wal_records(&encoded);
    let manifest = recovery_manifest(Lsn::new(1), Lsn::new(1));

    let plan =
        RecoveryPlan::from_manifest_and_wal_scan(&manifest, StartupMode::SafeStart, &scan).unwrap();

    assert_eq!(
        plan.wal_scan_stop().unwrap().reason,
        WalScanStopReason::TruncatedHeader
    );
    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
    assert_eq!(plan.incomplete_transactions.len(), 1);
    assert_eq!(plan.incomplete_transactions[0].transaction_id, tail_tx);
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(5))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

#[test]
fn redo_plan_exports_recovery_trace_with_durable_lsn_correlation() {
    let tx = TransactionId::new(30);
    let mut wal = InMemoryWal::new();
    wal.append_tx_begin(tx).unwrap();
    let row_lsn = wal
        .append_payload(WalRecordKind::RowUpdate, Some(tx), b"complete")
        .unwrap();
    wal.append_tx_commit(tx).unwrap();
    wal.flush_all().unwrap();

    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: row_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };
    let durable_records = wal.replay_durable();
    let plan =
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &durable_records)
            .unwrap();
    let trace_projection = plan.trace_projection(TraceId::new(50));
    let trace = ObserveRecoveryTrace {
        trace_id: trace_projection.trace_id,
        last_durable_lsn: trace_projection.last_durable_lsn,
        corruption_boundary_lsn: trace_projection.corruption_boundary_lsn,
    };

    let envelope = EventEnvelope::new(
        EventId::new(51),
        EventCorrelation {
            durable_lsn: Some(plan.durable_lsn.get()),
            ..EventCorrelation::empty()
        },
        TraceEvent::RecoveryStartup(trace),
    )
    .expect("recovery trace is correlated to the last durable WAL boundary");

    assert_eq!(
        envelope.correlation.durable_lsn,
        Some(plan.durable_lsn.get())
    );
}

#[test]
fn redo_plan_distinguishes_snapshot_replay_range_and_corruption_boundary() {
    let tx = TransactionId::new(40);
    let mut wal = InMemoryWal::new();
    wal.append_tx_begin(tx).unwrap();
    let row_lsn = wal
        .append_payload(WalRecordKind::RowInsert, Some(tx), b"truth")
        .unwrap();
    wal.append_tx_commit(tx).unwrap();
    wal.flush_all().unwrap();

    let manifest = DatabaseManifest {
        database_id: 7,
        manifest_version: 11,
        snapshot_id: 4242,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };
    let durable_records = wal.replay_durable();
    let clean_plan =
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &durable_records)
            .unwrap();

    assert_eq!(clean_plan.mounted_snapshot_id, manifest.snapshot_id);
    assert_eq!(clean_plan.redo_from_lsn, manifest.required_wal_start_lsn);
    assert!(clean_plan.durable_lsn >= row_lsn);
    assert_eq!(clean_plan.replay_lsns().collect::<Vec<_>>(), vec![row_lsn]);
    assert!(clean_plan.wal_scan_stop().is_none());

    let clean_trace = clean_plan.trace_projection(TraceId::new(901));
    assert_eq!(clean_trace.last_durable_lsn, clean_plan.durable_lsn.get());
    assert_eq!(
        clean_trace.corruption_boundary_lsn, None,
        "clean WAL scan must not synthesize a corruption boundary"
    );

    let tail_tx = TransactionId::new(41);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"durable".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(4),
            Some(Lsn::new(3)),
            Some(tail_tx),
            b"tail".to_vec(),
        )
        .unwrap(),
    ];
    let mut encoded = encode_records(&records);
    encoded.truncate(encoded.len() - 1);
    let scan = scan_wal_records(&encoded);

    let tail_plan =
        RecoveryPlan::from_manifest_and_wal_scan(&manifest, StartupMode::SafeStart, &scan)
            .expect("recoverable tail truncation must yield a plan, not an error");

    assert_eq!(tail_plan.mounted_snapshot_id, manifest.snapshot_id);
    assert_eq!(tail_plan.durable_lsn, Lsn::new(3));
    assert_eq!(
        tail_plan.replay_lsns().collect::<Vec<_>>(),
        vec![Lsn::new(2)]
    );
    assert_eq!(
        tail_plan.wal_scan_stop().unwrap().reason,
        WalScanStopReason::TruncatedRecord
    );

    let tail_trace = tail_plan.trace_projection(TraceId::new(902));
    assert_eq!(tail_trace.last_durable_lsn, tail_plan.durable_lsn.get());
    assert_eq!(
        tail_trace.corruption_boundary_lsn,
        Some(tail_plan.durable_lsn.get()),
        "recoverable WAL tail boundary must be projected as the corruption boundary LSN"
    );
}
