use andromeda_core::{AndromedaErrorKind, TransactionId};
use andromeda_observe::{EventCorrelation, EventEnvelope, EventId, TraceEvent, TraceId};
use andromeda_storage::format_version::{FormatVersion, StorageFormatKind};
use andromeda_storage::{
    DatabaseManifest, DurableTransactionState, InMemoryWal, Lsn, PreRedoStorageFormatDecision,
    PreRedoStorageFormatGate, PreRedoStorageFormatRejection, RECOVERY_REQUIRED_STORAGE_FORMATS,
    RecoveryPlan, RedoRecordDecision, StartupMode, StorageFormatFingerprint, StorageFormatManifest,
    WalRecord, WalRecordKind, WalScanStopReason, encode_wal_record, scan_wal_records,
};

#[test]
fn manifest_keeps_snapshot_plus_wal_anchor() {
    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(100),
        required_wal_start_lsn: Lsn::new(101),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };

    assert!(manifest.validate().is_ok());
    assert_eq!(
        RecoveryPlan::from_manifest(&manifest, StartupMode::SafeStart)
            .unwrap()
            .redo_from_lsn,
        Lsn::new(101)
    );
}

#[test]
fn redo_plan_replays_only_manifest_range_and_complete_transactions() {
    let incomplete_tx = TransactionId::new(21);
    let committed_tx = TransactionId::new(22);
    let mut wal = InMemoryWal::new();

    wal.append_tx_begin(incomplete_tx).unwrap();
    let incomplete_row_lsn = wal
        .append_payload(WalRecordKind::RowInsert, Some(incomplete_tx), b"incomplete")
        .unwrap();
    wal.append_tx_begin(committed_tx).unwrap();
    let committed_row_lsn = wal
        .append_payload(WalRecordKind::RowUpdate, Some(committed_tx), b"complete")
        .unwrap();
    wal.append_tx_commit(committed_tx).unwrap();
    wal.flush_all().unwrap();

    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: incomplete_row_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };
    let durable_records = wal.replay_durable();
    let plan =
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &durable_records)
            .unwrap();

    assert!(plan.has_incomplete_transactions());
    assert_eq!(
        plan.incomplete_transactions[0].transaction_id,
        incomplete_tx
    );
    assert_eq!(
        plan.replay_lsns().collect::<Vec<_>>(),
        vec![committed_row_lsn]
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == incomplete_row_lsn)
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

#[test]
fn redo_plan_skips_transaction_when_commit_is_not_durable() {
    let transaction_id = TransactionId::new(23);
    let mut wal = InMemoryWal::new();

    wal.append_tx_begin(transaction_id).unwrap();
    let row_lsn = wal
        .append_payload(
            WalRecordKind::RowDelete,
            Some(transaction_id),
            b"not-committed",
        )
        .unwrap();
    let commit_lsn = wal.append_tx_commit(transaction_id).unwrap();
    wal.flush_through(row_lsn).unwrap();

    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };
    let durable_records = wal.replay_durable();
    let plan =
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &durable_records)
            .unwrap();

    assert_eq!(commit_lsn, Lsn::new(3));
    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), Vec::<Lsn>::new());
    assert_eq!(plan.incomplete_transactions.len(), 1);
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == row_lsn)
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

#[test]
fn redo_plan_skips_transaction_with_commit_but_missing_begin_evidence() {
    let transaction_id = TransactionId::new(24);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::RowUpdate,
            Lsn::new(1),
            None,
            Some(transaction_id),
            b"missing-begin".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(transaction_id),
            Vec::new(),
        )
        .unwrap(),
    ];
    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };

    let plan =
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records).unwrap();

    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), Vec::<Lsn>::new());
    assert_eq!(
        plan.transaction_evidence[0].state,
        DurableTransactionState::Incomplete
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(1))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

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
    let mut encoded = Vec::new();
    for record in &records {
        encoded.extend(encode_wal_record(record).unwrap());
    }
    encoded.truncate(encoded.len() - 1);
    let scan = scan_wal_records(&encoded);
    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };

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
    let trace = plan.observe_recovery_trace(TraceId::new(50));

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
fn redo_plan_rejects_skipped_lsn_after_required_start() {
    let transaction_id = TransactionId::new(31);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(transaction_id),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(3),
            Some(Lsn::new(1)),
            Some(transaction_id),
            b"gap".to_vec(),
        )
        .unwrap(),
    ];
    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };

    assert_eq!(
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn redo_plan_rejects_duplicate_lsn_after_required_start() {
    let transaction_id = TransactionId::new(32);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(transaction_id),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(1),
            None,
            Some(transaction_id),
            Vec::new(),
        )
        .unwrap(),
    ];
    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };

    assert_eq!(
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn redo_plan_rejects_previous_lsn_mismatch_after_required_start() {
    let transaction_id = TransactionId::new(33);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(transaction_id),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(2),
            None,
            Some(transaction_id),
            Vec::new(),
        )
        .unwrap(),
    ];
    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };

    assert_eq!(
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
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

    let clean_trace = clean_plan.observe_recovery_trace(TraceId::new(901));
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
    let mut encoded = Vec::new();
    for record in &records {
        encoded.extend(encode_wal_record(record).unwrap());
    }
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

    let tail_trace = tail_plan.observe_recovery_trace(TraceId::new(902));
    assert_eq!(tail_trace.last_durable_lsn, tail_plan.durable_lsn.get());
    assert_eq!(
        tail_trace.corruption_boundary_lsn,
        Some(tail_plan.durable_lsn.get()),
        "recoverable WAL tail boundary must be projected as the corruption boundary LSN"
    );
}

#[test]
fn redo_plan_does_not_treat_non_durable_ram_records_as_truth() {
    let tx = TransactionId::new(60);
    let mut wal = InMemoryWal::new();
    wal.append_tx_begin(tx).unwrap();
    let _ram_only_row = wal
        .append_payload(WalRecordKind::RowInsert, Some(tx), b"ram-only")
        .unwrap();
    wal.append_tx_commit(tx).unwrap();

    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 0,
    };
    let durable_records = wal.replay_durable();
    assert!(
        durable_records.is_empty(),
        "non-flushed appends must not appear in the durable replay slice"
    );

    let result =
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &durable_records);
    assert_eq!(
        result.unwrap_err().kind(),
        AndromedaErrorKind::Storage,
        "missing required WAL start LSN must surface as a storage error",
    );
}

#[test]
fn pre_redo_format_gate_rejects_unknown_heap_layout_before_redo() {
    let observed = [
        StorageFormatFingerprint::new(StorageFormatKind::Page, FormatVersion::V1_0),
        StorageFormatFingerprint::new(StorageFormatKind::BTreeKey, FormatVersion::V1_0),
        StorageFormatFingerprint::new(StorageFormatKind::BTreeNode, FormatVersion::V1_0),
        StorageFormatFingerprint::new(StorageFormatKind::WalPayload, FormatVersion::V1_0),
    ];

    assert_eq!(
        PreRedoStorageFormatGate::decide(
            StartupMode::SafeStart,
            false,
            RECOVERY_REQUIRED_STORAGE_FORMATS,
            &observed,
        ),
        Err(PreRedoStorageFormatRejection::Unknown {
            kind: StorageFormatKind::HeapPage
        })
    );
}

#[test]
fn pre_redo_format_gate_rejects_unsupported_btree_key_version() {
    let observed =
        recovery_v1_format_fingerprints_with(StorageFormatKind::BTreeKey, FormatVersion::V1_5);

    assert_eq!(
        PreRedoStorageFormatGate::decide(
            StartupMode::FastStart,
            false,
            RECOVERY_REQUIRED_STORAGE_FORMATS,
            &observed,
        ),
        Err(PreRedoStorageFormatRejection::Unsupported {
            kind: StorageFormatKind::BTreeKey,
            observed: FormatVersion::V1_5,
            reader: FormatVersion::V1_0,
        })
    );
}

#[test]
fn forensic_start_opens_unknown_format_read_only_without_replay() {
    let observed = [
        StorageFormatFingerprint::new(StorageFormatKind::Page, FormatVersion::V1_0),
        StorageFormatFingerprint::new(StorageFormatKind::BTreeKey, FormatVersion::V1_0),
        StorageFormatFingerprint::new(StorageFormatKind::BTreeNode, FormatVersion::V1_0),
        StorageFormatFingerprint::new(StorageFormatKind::WalPayload, FormatVersion::V1_0),
    ];

    let decision = PreRedoStorageFormatGate::decide(
        StartupMode::ForensicStart,
        true,
        RECOVERY_REQUIRED_STORAGE_FORMATS,
        &observed,
    )
    .unwrap();

    assert_eq!(
        decision,
        PreRedoStorageFormatDecision::ForensicReadOnly {
            reason: PreRedoStorageFormatRejection::Unknown {
                kind: StorageFormatKind::HeapPage
            }
        }
    );
    assert!(
        !decision.replay_allowed(),
        "ForensicStart may inspect unknown storage formats only with replay disabled"
    );
}

#[test]
fn recovery_plan_format_fingerprint_api_fails_before_wal_redo_validation() {
    let transaction_id = TransactionId::new(70);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(transaction_id),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(1)),
            Some(transaction_id),
            Vec::new(),
        )
        .unwrap(),
    ];
    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };
    let observed =
        recovery_v1_format_fingerprints_with(StorageFormatKind::HeapPage, FormatVersion::V2_0);

    let err = RecoveryPlan::from_manifest_wal_and_format_fingerprints(
        &manifest,
        StartupMode::SafeStart,
        &records,
        &observed,
    )
    .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(
        err.message()
            .contains("unsupported storage subformat `Heap Page`"),
        "format gate must fail before WAL redo validation; got: {}",
        err.message()
    );
}

#[test]
fn recovery_plan_reads_and_validates_format_fingerprints_from_manifest_before_redo() {
    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::PageAllocate,
            Lsn::new(1),
            None,
            None,
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::PageFormat,
            Lsn::new(3),
            Some(Lsn::new(1)),
            None,
            Vec::new(),
        )
        .unwrap(),
    ];
    let storage_manifest = StorageFormatManifest::new(
        manifest.database_id,
        manifest.manifest_version,
        manifest.snapshot_id,
        recovery_v1_format_fingerprints_with(StorageFormatKind::HeapPage, FormatVersion::V2_0),
    );

    let err = RecoveryPlan::from_manifest_wal_and_storage_format_manifest(
        &manifest,
        StartupMode::SafeStart,
        &records,
        &storage_manifest,
    )
    .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(
        err.message()
            .contains("unsupported storage subformat `Heap Page`"),
        "manifest-backed format gate must fail before WAL coverage validation; got: {}",
        err.message()
    );
}

#[test]
fn recovery_plan_rejects_tampered_storage_format_manifest_before_redo() {
    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::PageFormat,
            Lsn::new(1),
            None,
            None,
            Vec::new(),
        )
        .unwrap(),
    ];
    let mut storage_manifest = manifest.storage_format_manifest().unwrap();
    storage_manifest.fingerprints[0].version = FormatVersion::V1_5;

    let err = RecoveryPlan::from_manifest_wal_and_storage_format_manifest(
        &manifest,
        StartupMode::SafeStart,
        &records,
        &storage_manifest,
    )
    .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(
        err.message().contains("fingerprint hash mismatch"),
        "tampered manifest fingerprints must be rejected before redo; got: {}",
        err.message()
    );
}

fn recovery_v1_format_fingerprints_with(
    override_kind: StorageFormatKind,
    override_version: FormatVersion,
) -> Vec<StorageFormatFingerprint> {
    RECOVERY_REQUIRED_STORAGE_FORMATS
        .iter()
        .map(|kind| {
            StorageFormatFingerprint::new(
                *kind,
                if *kind == override_kind {
                    override_version
                } else {
                    FormatVersion::V1_0
                },
            )
        })
        .collect()
}
