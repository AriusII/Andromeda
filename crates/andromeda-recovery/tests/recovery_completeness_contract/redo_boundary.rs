use super::support::manifest_for_replay_from;
use crate::support::replay_wal_from_lsn_into_context;
use andromeda_recovery::{ReplayContext, StartupMode};
use andromeda_wal::{Lsn, WalRecord, WalRecordKind};

#[test]
fn zero_redo_boundary_is_allowed_only_for_explicit_bootstrap() {
    let bootstrap = manifest_for_replay_from(Lsn::ZERO);
    let report = replay_wal_from_lsn_into_context(
        &bootstrap,
        StartupMode::SafeStart,
        &[],
        &mut ReplayContext::new(),
    )
    .expect("empty bootstrap recovery should be explicit and valid");
    assert_eq!(report.replay_start_lsn, Lsn::ZERO);
    assert_eq!(report.total_records, 0);

    let first_epoch_records = vec![
        WalRecord::from_parts(
            WalRecordKind::SecurityAuditAppend,
            Lsn::new(1),
            None,
            None,
            Vec::new(),
        )
        .expect("first epoch record should be valid"),
    ];
    let report = replay_wal_from_lsn_into_context(
        &bootstrap,
        StartupMode::SafeStart,
        &first_epoch_records,
        &mut ReplayContext::new(),
    )
    .expect("bootstrap recovery may replay a WAL chain that starts at genesis LSN 1");
    assert_eq!(report.total_records, 1);
    assert_eq!(report.handler_skipped_count, 1);
}

#[test]
fn zero_redo_boundary_rejects_non_bootstrap_snapshot_or_missing_wal_prefix() {
    let mut snapshot_manifest = manifest_for_replay_from(Lsn::ZERO);
    snapshot_manifest.base_checkpoint_lsn = Lsn::new(40);

    let err = replay_wal_from_lsn_into_context(
        &snapshot_manifest,
        StartupMode::SafeStart,
        &[],
        &mut ReplayContext::new(),
    )
    .expect_err("non-bootstrap snapshots must not use ZERO as redo boundary");
    assert!(err.message().contains("bootstrap ZERO redo boundary"));

    let bootstrap = manifest_for_replay_from(Lsn::ZERO);
    let truncated_prefix = vec![
        WalRecord::from_parts(
            WalRecordKind::SecurityAuditAppend,
            Lsn::new(5),
            Some(Lsn::new(4)),
            None,
            Vec::new(),
        )
        .expect("record should be valid enough for boundary validation"),
    ];

    let err = replay_wal_from_lsn_into_context(
        &bootstrap,
        StartupMode::SafeStart,
        &truncated_prefix,
        &mut ReplayContext::new(),
    )
    .expect_err("ZERO boundary must not hide missing durable WAL prefix");

    assert!(err.message().contains("start at LSN 1"));
}
