use crate::support::recovery_manifest;
use andromeda_core::{AndromedaErrorKind, TransactionId};
use andromeda_storage::{Lsn, RecoveryPlan, StartupMode, WalRecord, WalRecordKind};

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
    let manifest = recovery_manifest(Lsn::new(1), Lsn::new(1));

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
    let manifest = recovery_manifest(Lsn::new(1), Lsn::new(1));

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
    let manifest = recovery_manifest(Lsn::new(1), Lsn::new(1));

    assert_eq!(
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );
}
