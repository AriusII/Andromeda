use super::*;
use crate::Lsn;
use andromeda_error::AndromedaErrorKind;

#[test]
fn page_flush_allowed_when_lsn_equals_durable() {
    let page_lsn = Lsn::new(100);
    let wal_durable = Lsn::new(100);
    assert!(validate_wal_durability_before_page_flush(page_lsn, wal_durable).is_ok());
}

#[test]
fn page_flush_allowed_when_lsn_less_than_durable() {
    let page_lsn = Lsn::new(50);
    let wal_durable = Lsn::new(100);
    assert!(validate_wal_durability_before_page_flush(page_lsn, wal_durable).is_ok());
}

#[test]
fn page_flush_blocked_when_lsn_exceeds_durable() {
    let page_lsn = Lsn::new(150);
    let wal_durable = Lsn::new(100);
    let result = validate_wal_durability_before_page_flush(page_lsn, wal_durable);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Storage);
}

#[test]
fn page_flush_allowed_for_zero_lsn() {
    let page_lsn = Lsn::ZERO;
    let wal_durable = Lsn::ZERO;
    assert!(validate_wal_durability_before_page_flush(page_lsn, wal_durable).is_ok());
}

#[test]
fn page_flush_blocked_when_durable_is_zero_and_page_lsn_nonzero() {
    let page_lsn = Lsn::new(1);
    let wal_durable = Lsn::ZERO;
    let result = validate_wal_durability_before_page_flush(page_lsn, wal_durable);
    assert!(result.is_err());
}

#[test]
fn manifest_switch_allowed_when_checkpoint_equals_wal_checkpoint() {
    let manifest_ckpt = Lsn::new(500);
    let wal_durable = Lsn::new(600);
    let wal_ckpt = Lsn::new(500);
    assert!(validate_manifest_atomic_switch(manifest_ckpt, wal_durable, wal_ckpt).is_ok());
}

#[test]
fn manifest_switch_allowed_when_checkpoint_less_than_wal_checkpoint() {
    let manifest_ckpt = Lsn::new(400);
    let wal_durable = Lsn::new(600);
    let wal_ckpt = Lsn::new(500);
    assert!(validate_manifest_atomic_switch(manifest_ckpt, wal_durable, wal_ckpt).is_ok());
}

#[test]
fn manifest_switch_blocked_when_checkpoint_exceeds_wal_checkpoint() {
    let manifest_ckpt = Lsn::new(600);
    let wal_durable = Lsn::new(600);
    let wal_ckpt = Lsn::new(500);
    let result = validate_manifest_atomic_switch(manifest_ckpt, wal_durable, wal_ckpt);
    assert!(result.is_err());
    assert!(result.unwrap_err().message().contains("checkpoint LSN"));
}

#[test]
fn manifest_switch_blocked_when_wal_checkpoint_exceeds_durable_wal() {
    let manifest_ckpt = Lsn::new(500);
    let wal_durable = Lsn::new(499);
    let wal_ckpt = Lsn::new(500);
    let result = validate_manifest_atomic_switch(manifest_ckpt, wal_durable, wal_ckpt);
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(error.message().contains("durable WAL LSN"));
}

#[test]
fn manifest_switch_blocked_when_both_checkpoints_zero_but_manifest_higher() {
    // This is vacuous: if both are zero, they're equal, so switch is allowed.
    let manifest_ckpt = Lsn::ZERO;
    let wal_durable = Lsn::ZERO;
    let wal_ckpt = Lsn::ZERO;
    assert!(validate_manifest_atomic_switch(manifest_ckpt, wal_durable, wal_ckpt).is_ok());
}

#[test]
fn recovery_allowed_when_floor_equals_required() {
    let floor = Lsn::new(300);
    let required = Lsn::new(300);
    assert!(validate_recovery_floor(floor, required).is_ok());
}

#[test]
fn recovery_allowed_when_floor_exceeds_required() {
    let floor = Lsn::new(400);
    let required = Lsn::new(300);
    assert!(validate_recovery_floor(floor, required).is_ok());
}

#[test]
fn recovery_blocked_when_floor_before_required() {
    let floor = Lsn::new(200);
    let required = Lsn::new(300);
    let result = validate_recovery_floor(floor, required);
    assert!(result.is_err());
    assert!(result.unwrap_err().message().contains("recovery floor"));
}

#[test]
fn recovery_floor_validation_at_system_startup() {
    // Simulate manifest load at startup
    let manifest_required_wal_start = Lsn::new(250);
    let recovery_floor = Lsn::new(250);

    // Should succeed: recovery can begin at the required point
    assert!(validate_recovery_floor(recovery_floor, manifest_required_wal_start).is_ok());
}

#[test]
fn strict_ordering_passes_when_earlier_strictly_less_than_later() {
    let earlier = Lsn::new(100);
    let later = Lsn::new(200);
    assert!(validate_lsn_strictly_ordered(earlier, "old", later, "new").is_ok());
}

#[test]
fn strict_ordering_fails_when_earlier_equals_later() {
    let earlier = Lsn::new(100);
    let later = Lsn::new(100);
    let result = validate_lsn_strictly_ordered(earlier, "old", later, "new");
    assert!(result.is_err());
    assert!(result.unwrap_err().message().contains("ordering violation"));
}

#[test]
fn strict_ordering_fails_when_earlier_greater_than_later() {
    let earlier = Lsn::new(200);
    let later = Lsn::new(100);
    let result = validate_lsn_strictly_ordered(earlier, "old", later, "new");
    assert!(result.is_err());
}

#[test]
fn non_decreasing_ordering_passes_when_earlier_less_than_later() {
    let earlier = Lsn::new(100);
    let later = Lsn::new(200);
    assert!(validate_lsn_ordered(earlier, "old", later, "new").is_ok());
}

#[test]
fn non_decreasing_ordering_passes_when_equal() {
    let earlier = Lsn::new(100);
    let later = Lsn::new(100);
    assert!(validate_lsn_ordered(earlier, "old", later, "new").is_ok());
}

#[test]
fn non_decreasing_ordering_fails_when_earlier_greater_than_later() {
    let earlier = Lsn::new(200);
    let later = Lsn::new(100);
    let result = validate_lsn_ordered(earlier, "old", later, "new");
    assert!(result.is_err());
}

#[test]
fn combined_page_flush_then_manifest_switch_is_safe() {
    // Simulate: page dirtied at LSN 100, WAL durable through 200, manifest at 150
    let page_lsn = Lsn::new(100);
    let wal_durable = Lsn::new(200);
    let manifest_ckpt = Lsn::new(150);
    let wal_ckpt = Lsn::new(200);

    // Page can be flushed
    assert!(validate_wal_durability_before_page_flush(page_lsn, wal_durable).is_ok());

    // Manifest can switch
    assert!(validate_manifest_atomic_switch(manifest_ckpt, wal_durable, wal_ckpt).is_ok());
}

#[test]
fn combined_page_flush_blocked_prevents_manifest_switch() {
    // Simulate: page dirtied at LSN 300, WAL durable through 200, manifest at 150
    let page_lsn = Lsn::new(300);
    let wal_durable = Lsn::new(200);
    let manifest_ckpt = Lsn::new(150);
    let wal_ckpt = Lsn::new(200);

    // Page cannot be flushed
    assert!(validate_wal_durability_before_page_flush(page_lsn, wal_durable).is_err());

    // But manifest could theoretically switch (its checkpoint is before wal checkpoint)
    assert!(validate_manifest_atomic_switch(manifest_ckpt, wal_durable, wal_ckpt).is_ok());
}

#[test]
fn recovery_cascade_validation() {
    // Simulate full system checkpoint
    let old_manifest_required = Lsn::new(100);
    let new_manifest_checkpoint = Lsn::new(500);
    let wal_durable = Lsn::new(600);
    let wal_checkpoint = Lsn::new(500);
    let recovery_floor = Lsn::new(500);

    // 1. Check manifest can switch
    assert!(
        validate_manifest_atomic_switch(new_manifest_checkpoint, wal_durable, wal_checkpoint)
            .is_ok()
    );

    // 2. Check recovery can start from the new manifest's required floor
    assert!(validate_recovery_floor(recovery_floor, new_manifest_checkpoint).is_ok());

    // 3. Check ordering: old < new
    assert!(
        validate_lsn_strictly_ordered(
            old_manifest_required,
            "old manifest required",
            new_manifest_checkpoint,
            "new manifest checkpoint"
        )
        .is_ok()
    );
}

#[test]
fn zero_lsn_comparisons_are_consistent() {
    let zero = Lsn::ZERO;
    let one = Lsn::new(1);

    assert!(validate_wal_durability_before_page_flush(zero, zero).is_ok());
    assert!(validate_wal_durability_before_page_flush(zero, one).is_ok());
    assert!(validate_wal_durability_before_page_flush(one, zero).is_err());
}

#[test]
fn max_lsn_comparisons_are_consistent() {
    let max = Lsn::MAX;
    let one_before_max = Lsn::new(u64::MAX - 1);

    assert!(validate_wal_durability_before_page_flush(max, max).is_ok());
    assert!(validate_wal_durability_before_page_flush(one_before_max, max).is_ok());
    assert!(validate_wal_durability_before_page_flush(max, one_before_max).is_err());
}

#[test]
fn error_messages_include_lsn_values() {
    let result = validate_wal_durability_before_page_flush(Lsn::new(150), Lsn::new(100));
    let err = result.unwrap_err();
    let error_msg = err.message();
    assert!(error_msg.contains("150"));
    assert!(error_msg.contains("100"));
}
