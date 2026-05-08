use crate::support::{
    assert_lsn_strictly_ordered, assert_manifest_switch_allowed, assert_page_flush_allowed,
    assert_recovery_floor_allowed,
};
use andromeda_wal::Lsn;

#[test]
fn crash_at_manifest_switch_requires_checkpoint_and_recovery_floor_checks() {
    let old_manifest_checkpoint = Lsn::new(300);
    let new_manifest_checkpoint = Lsn::new(500);
    let new_manifest_required_wal_start = Lsn::new(300);
    let wal_durable = Lsn::new(550);
    let wal_checkpoint = Lsn::new(500);
    let recovery_floor = Lsn::new(300);

    assert_manifest_switch_allowed(
        new_manifest_checkpoint,
        wal_durable,
        wal_checkpoint,
        "New manifest checkpoint is safe",
    );
    assert_recovery_floor_allowed(
        recovery_floor,
        new_manifest_required_wal_start,
        "Recovery floor is sufficient",
    );
    assert_lsn_strictly_ordered(
        old_manifest_checkpoint,
        "old_checkpoint",
        new_manifest_checkpoint,
        "new_checkpoint",
        "Manifest versions monotonically increase",
    );
}

#[test]
fn full_checkpoint_workflow_preserves_manifest_and_recovery_floor() {
    let page_lsns = [Lsn::new(100), Lsn::new(150), Lsn::new(200)];
    let wal_durable_after_logging = Lsn::new(300);
    let checkpoint_lsn = Lsn::new(300);
    let recovery_floor = Lsn::new(300);

    for page_lsn in &page_lsns {
        assert_page_flush_allowed(
            *page_lsn,
            wal_durable_after_logging,
            "All pages should be flushable after WAL logs all changes",
        );
    }

    assert_manifest_switch_allowed(
        checkpoint_lsn,
        wal_durable_after_logging,
        checkpoint_lsn,
        "Manifest should increment after checkpoint",
    );
    assert_recovery_floor_allowed(
        recovery_floor,
        checkpoint_lsn,
        "Recovery floor is valid for new manifest",
    );
}

#[test]
fn multiple_checkpoints_advance_recovery_floor_monotonically() {
    let checkpoints = [
        (Lsn::new(100), Lsn::new(100)),
        (Lsn::new(300), Lsn::new(300)),
        (Lsn::new(600), Lsn::new(600)),
    ];

    for i in 1..checkpoints.len() {
        let prev_checkpoint = checkpoints[i - 1].0;
        let curr_checkpoint = checkpoints[i].0;
        let recovery_floor = curr_checkpoint;

        assert_lsn_strictly_ordered(
            prev_checkpoint,
            "prev",
            curr_checkpoint,
            "curr",
            "Checkpoints should strictly increase",
        );
        assert_recovery_floor_allowed(
            recovery_floor,
            curr_checkpoint,
            "Recovery floor should remain sufficient after checkpoint",
        );
    }
}
