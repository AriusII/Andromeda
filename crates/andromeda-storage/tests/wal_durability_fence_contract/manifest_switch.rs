use crate::support::{assert_manifest_switch_allowed, assert_message_contains};
use andromeda_storage::{Lsn, validate_manifest_atomic_switch};

#[test]
fn manifest_can_switch_when_checkpoint_equals_wal_checkpoint() {
    assert_manifest_switch_allowed(
        Lsn::new(500),
        Lsn::new(600),
        Lsn::new(500),
        "Manifest should switch when checkpoint matches WAL checkpoint",
    );
}

#[test]
fn manifest_can_switch_when_checkpoint_precedes_wal_checkpoint() {
    assert_manifest_switch_allowed(
        Lsn::new(400),
        Lsn::new(600),
        Lsn::new(500),
        "Manifest should switch when checkpoint < WAL checkpoint",
    );
}

#[test]
fn manifest_blocked_from_switch_when_checkpoint_exceeds_wal_checkpoint() {
    let result = validate_manifest_atomic_switch(Lsn::new(600), Lsn::new(600), Lsn::new(500));
    assert!(
        result.is_err(),
        "Manifest should be blocked when checkpoint > WAL checkpoint"
    );

    let error = result.unwrap_err();
    assert_message_contains(error.message(), "manifest", "Error should mention manifest");
}

#[test]
fn manifest_blocked_from_switch_when_wal_checkpoint_exceeds_durable_wal() {
    let result = validate_manifest_atomic_switch(Lsn::new(500), Lsn::new(500), Lsn::new(600));
    assert!(
        result.is_err(),
        "Manifest should be blocked when WAL checkpoint evidence is not durable"
    );

    let error = result.unwrap_err();
    assert_message_contains(
        error.message(),
        "durable WAL LSN",
        "Error should mention the durable WAL boundary",
    );
}

#[test]
fn manifest_zero_checkpoint_switch_is_bootstrap_only() {
    assert_manifest_switch_allowed(
        Lsn::ZERO,
        Lsn::ZERO,
        Lsn::ZERO,
        "ZERO checkpoint switch is only valid for empty bootstrap state",
    );

    let result = validate_manifest_atomic_switch(Lsn::ZERO, Lsn::new(10), Lsn::new(10));
    assert!(
        result.is_err(),
        "manifest switch must not publish a ZERO checkpoint after durable WAL exists"
    );
    assert_message_contains(
        result.unwrap_err().message(),
        "bootstrap manifest checkpoint",
        "Error should mention bootstrap checkpoint policy",
    );
}

#[test]
fn manifest_switch_scenario_after_checkpoint_operation() {
    let checkpoint_lsn = Lsn::new(500);

    assert_manifest_switch_allowed(
        checkpoint_lsn,
        checkpoint_lsn,
        checkpoint_lsn,
        "Manifest should switch after WAL checkpoint",
    );
}

#[test]
fn manifest_atomic_switch_error_includes_lsn_details() {
    let error =
        validate_manifest_atomic_switch(Lsn::new(700), Lsn::new(700), Lsn::new(600)).unwrap_err();

    let msg = error.message();
    assert_message_contains(msg, "700", "Error should include manifest checkpoint LSN");
    assert_message_contains(msg, "600", "Error should include WAL checkpoint LSN");
}
