use crate::support::*;
use andromeda_storage::{
    Lsn, PitrTarget, WalArchiveRange, plan_replay_segments, validate_pitr_target,
    validate_restore_prerequisites,
};

#[test]
fn test_pitr_lsn_within_range() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(1500);

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_ok());
}

#[test]
fn test_pitr_lsn_at_range_start() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(1001);

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_ok());
}

#[test]
fn test_pitr_lsn_at_snapshot_base_checkpoint_is_snapshot_only() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(1000);

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_ok());
    let plan = plan_replay_segments(&manifest, pitr_lsn, &[])
        .expect("snapshot base checkpoint target must not require WAL replay");
    assert!(plan.is_empty());
}

#[test]
fn test_pitr_lsn_at_range_end() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(2000);

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_ok());
}

#[test]
fn test_pitr_lsn_below_range() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(999);

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_err());
}

#[test]
fn test_pitr_lsn_above_range() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(2001);

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_err());
}

#[test]
fn test_restore_prerequisites_reject_wal_archive_after_snapshot_required_start() {
    let mut manifest = make_test_manifest();
    manifest.wal_archive = WalArchiveRange::new(Lsn::new(1002), Lsn::new(2000));

    let err = validate_restore_prerequisites(&manifest, Lsn::new(1500))
        .expect_err("archive must anchor at snapshot required WAL start");

    assert!(
        err.message().contains("required WAL start"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_restore_prerequisites_reject_pitr_before_snapshot_required_start() {
    let mut manifest = make_test_manifest();
    manifest.snapshot.required_wal_start_lsn = Lsn::new(1005);
    manifest.wal_archive = WalArchiveRange::new(Lsn::new(900), Lsn::new(2000));

    let err = validate_restore_prerequisites(&manifest, Lsn::new(1002))
        .expect_err("PITR before snapshot required WAL start must fail closed");

    assert!(
        err.message().contains("required WAL start"),
        "unexpected error: {err}"
    );
}

#[test]
fn backup_pitr_validator_rejects_target_before_required_wal_start() {
    let mut manifest = make_test_manifest();
    manifest.snapshot.required_wal_start_lsn = Lsn::new(1005);
    manifest.wal_archive = WalArchiveRange::new(Lsn::new(900), Lsn::new(2000));

    let err = validate_pitr_target(&manifest, PitrTarget::new(Lsn::new(1002)))
        .expect_err("PITR before required WAL start must fail closed");

    assert!(
        err.message().contains("required WAL start"),
        "unexpected error: {err}"
    );
}
