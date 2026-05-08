use crate::support::{
    assert_lsn_strictly_ordered, assert_message_contains, assert_recovery_floor_allowed,
};
use andromeda_wal::{Lsn, validate_lsn_strictly_ordered, validate_recovery_floor};

#[test]
fn recovery_allowed_when_floor_equals_required() {
    assert_recovery_floor_allowed(
        Lsn::new(300),
        Lsn::new(300),
        "Recovery should be allowed when floor equals required",
    );
}

#[test]
fn recovery_allowed_when_floor_exceeds_required() {
    assert_recovery_floor_allowed(
        Lsn::new(400),
        Lsn::new(300),
        "Recovery should be allowed when floor > required",
    );
}

#[test]
fn recovery_blocked_when_floor_precedes_required() {
    let result = validate_recovery_floor(Lsn::new(300), Lsn::new(400));
    assert!(
        result.is_err(),
        "Recovery should be blocked when floor < required"
    );

    let error = result.unwrap_err();
    assert_message_contains(
        error.message(),
        "recovery floor",
        "Error should mention recovery floor",
    );
}

#[test]
fn lsn_monotonicity_enforced_across_checkpoint_versions() {
    assert_lsn_strictly_ordered(
        Lsn::new(100),
        "checkpoint_v1",
        Lsn::new(200),
        "checkpoint_v2",
        "Checkpoints should strictly increase",
    );
}

#[test]
fn lsn_strict_ordering_rejects_equal_lsns() {
    let result = validate_lsn_strictly_ordered(Lsn::new(500), "old", Lsn::new(500), "new");
    assert!(result.is_err(), "Strict ordering should reject equal LSNs");
}
