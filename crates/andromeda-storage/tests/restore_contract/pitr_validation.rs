use crate::support::*;
use andromeda_backup::WalArchiveRange;
use andromeda_restore::{
    PitrTarget, PitrTargetRejection, plan_replay_segments, validate_pitr_target,
    validate_pitr_target_with_audit, validate_restore_prerequisites,
};
use andromeda_wal::Lsn;

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

#[test]
fn backup_pitr_validator_reports_auditable_target_bounds() {
    let manifest = make_test_manifest();

    let (accepted, audit) =
        validate_pitr_target_with_audit(&manifest, PitrTarget::new(Lsn::new(1000)));
    let accepted = accepted.expect("snapshot base checkpoint target should be accepted");
    assert!(accepted.replay_skipped);
    assert!(!accepted.requires_wal_replay());
    assert!(audit.accepted);
    assert_eq!(audit.rejection, None);
    assert_eq!(audit.target_lsn, Lsn::new(1000));

    let (zero, audit) = validate_pitr_target_with_audit(&manifest, PitrTarget::new(Lsn::new(0)));
    assert!(zero.is_err());
    assert!(!audit.accepted);
    assert_eq!(audit.rejection, Some(PitrTargetRejection::TargetLsnZero));

    let (before_snapshot, audit) =
        validate_pitr_target_with_audit(&manifest, PitrTarget::new(Lsn::new(999)));
    assert!(before_snapshot.is_err());
    assert_eq!(
        audit.rejection,
        Some(PitrTargetRejection::TargetBeforeSnapshot)
    );

    let mut gap_manifest = make_test_manifest();
    gap_manifest.snapshot.required_wal_start_lsn = Lsn::new(1005);
    gap_manifest.wal_archive = WalArchiveRange::new(Lsn::new(1001), Lsn::new(2000));
    let (before_required_start, audit) =
        validate_pitr_target_with_audit(&gap_manifest, PitrTarget::new(Lsn::new(1002)));
    assert!(before_required_start.is_err());
    assert_eq!(
        audit.rejection,
        Some(PitrTargetRejection::TargetBeforeRequiredWalStart)
    );

    let (beyond_archive, audit) =
        validate_pitr_target_with_audit(&manifest, PitrTarget::new(Lsn::new(2001)));
    assert!(beyond_archive.is_err());
    assert_eq!(
        audit.rejection,
        Some(PitrTargetRejection::TargetBeyondWalRange)
    );
}
