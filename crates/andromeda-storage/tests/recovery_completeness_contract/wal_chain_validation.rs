use super::support::manifest_for_replay_from;
use andromeda_core::TransactionId;
use andromeda_storage::{
    Lsn, ReplayContext, StartupMode, WalRecord, WalRecordKind, replay_wal_from_lsn_into_context,
};

#[test]
fn wal_replay_rejects_duplicate_or_reordered_lsn_before_visibility() {
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::SecurityAuditAppend,
            Lsn::new(1),
            None,
            None,
            Vec::new(),
        )
        .expect("first record should be valid"),
        WalRecord::from_parts(
            WalRecordKind::SecurityAuditAppend,
            Lsn::new(1),
            None,
            None,
            Vec::new(),
        )
        .expect("duplicate LSN record should be structurally valid"),
    ];

    let err = replay_wal_from_lsn_into_context(
        &manifest_for_replay_from(Lsn::new(1)),
        StartupMode::SafeStart,
        &records,
        &mut ReplayContext::new(),
    )
    .expect_err("recovery must reject duplicate LSNs before replay");

    assert!(
        err.message().contains("strictly increasing by LSN"),
        "error must identify the monotonic LSN invariant: {}",
        err.message()
    );
}

#[test]
fn wal_replay_rejects_previous_lsn_chain_mismatch_before_visibility() {
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::SecurityAuditAppend,
            Lsn::new(1),
            None,
            None,
            Vec::new(),
        )
        .expect("first record should be valid"),
        WalRecord::from_parts(
            WalRecordKind::SecurityAuditAppend,
            Lsn::new(2),
            None,
            None,
            Vec::new(),
        )
        .expect("wrong previous LSN record should be structurally valid"),
    ];

    let err = replay_wal_from_lsn_into_context(
        &manifest_for_replay_from(Lsn::new(1)),
        StartupMode::SafeStart,
        &records,
        &mut ReplayContext::new(),
    )
    .expect_err("recovery must reject broken previous-LSN chains before replay");

    assert!(
        err.message().contains("previous LSN chain mismatch"),
        "error must identify the WAL chain invariant: {}",
        err.message()
    );
}

#[test]
fn wal_replay_rejects_conflicting_terminal_records_before_visibility() {
    let tx = TransactionId::new(45);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .expect("begin record should be valid"),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            Vec::new(),
        )
        .expect("commit record should be valid"),
        WalRecord::from_parts(
            WalRecordKind::TxRollback,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            Vec::new(),
        )
        .expect("rollback record should be valid"),
    ];

    let err = replay_wal_from_lsn_into_context(
        &manifest_for_replay_from(Lsn::new(1)),
        StartupMode::SafeStart,
        &records,
        &mut ReplayContext::new(),
    )
    .expect_err("conflicting terminal records must fail recovery closed");

    assert!(
        err.message()
            .contains("duplicate or conflicting terminal records"),
        "error must expose conflicting terminal evidence: {}",
        err.message()
    );
    assert!(
        err.message().contains("TxCommit") && err.message().contains("TxRollback"),
        "error must name both terminal record kinds: {}",
        err.message()
    );
}
