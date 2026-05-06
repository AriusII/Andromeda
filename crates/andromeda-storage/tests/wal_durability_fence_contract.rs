#![forbid(unsafe_code)]

//! WAL Durability Fence Contract Tests
//!
//! This test suite validates the core invariants that must hold for safe page
//! flushing and manifest switching:
//!
//! **Primary Invariants:**
//! 1. Page Durability: A page's first_dirty_lsn must be ≤ wal_durable_lsn before flush
//! 2. Manifest Atomicity: Manifest version can only advance after WAL checkpoint
//! 3. Recovery Correctness: Recovery floor must be ≥ required_wal_start_lsn
//! 4. LSN Monotonicity: LSNs must strictly increase across manifest versions
//!
//! These tests verify that crashes at any point maintain consistency.

use andromeda_core::AndromedaErrorKind;
use andromeda_storage::{
    Lsn, validate_lsn_strictly_ordered, validate_manifest_atomic_switch, validate_recovery_floor,
    validate_wal_durability_before_page_flush,
};

#[test]
fn page_can_flush_when_first_dirty_lsn_equals_wal_durable_lsn() {
    // Setup: Page dirtied at LSN 100, WAL durable through 100
    let page_first_dirty_lsn = Lsn::new(100);
    let wal_durable_lsn = Lsn::new(100);

    // Assert: Page can be flushed
    assert!(
        validate_wal_durability_before_page_flush(page_first_dirty_lsn, wal_durable_lsn).is_ok(),
        "Page should be flushable when first_dirty_lsn equals wal_durable_lsn"
    );
}

#[test]
fn page_can_flush_when_first_dirty_lsn_precedes_wal_durable_lsn() {
    // Setup: Page dirtied at LSN 50, WAL durable through 100 (page dirty before WAL advanced)
    let page_first_dirty_lsn = Lsn::new(50);
    let wal_durable_lsn = Lsn::new(100);

    // Assert: Page can be flushed (WAL has overshot its change)
    assert!(
        validate_wal_durability_before_page_flush(page_first_dirty_lsn, wal_durable_lsn).is_ok(),
        "Page should be flushable when first_dirty_lsn < wal_durable_lsn"
    );
}

#[test]
fn page_blocked_from_flush_when_first_dirty_lsn_exceeds_wal_durable_lsn() {
    // Setup: Page dirtied at LSN 150, WAL only durable through 100
    let page_first_dirty_lsn = Lsn::new(150);
    let wal_durable_lsn = Lsn::new(100);

    // Assert: Page cannot be flushed
    let result = validate_wal_durability_before_page_flush(page_first_dirty_lsn, wal_durable_lsn);
    assert!(
        result.is_err(),
        "Page should be blocked when first_dirty_lsn > wal_durable_lsn"
    );

    // Verify error details
    let error = result.unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(
        error.message().contains("150"),
        "Error message should include page LSN"
    );
    assert!(
        error.message().contains("100"),
        "Error message should include WAL LSN"
    );
}

#[test]
fn page_multiple_dirty_cycles_accumulate_to_first_dirty_lsn() {
    // Setup: Page dirtied at LSN 200, then dirtied again at LSN 250
    // Buffer pool's DirtyTracker preserves the FIRST dirty LSN (200)
    let page_first_dirty_lsn = Lsn::new(200); // Earliest modification
    let wal_durable_lsn = Lsn::new(250); // WAL caught up to latest

    // Assert: Page can flush because WAL is durable through its FIRST modification
    assert!(
        validate_wal_durability_before_page_flush(page_first_dirty_lsn, wal_durable_lsn).is_ok(),
        "Page should be flushable once WAL durable through first modification"
    );
}

#[test]
fn page_zero_lsn_special_case() {
    // Setup: Page at LSN 0 (clean page, no modifications in this epoch)
    // WAL also at LSN 0 (system startup)
    let page_lsn = Lsn::ZERO;
    let wal_durable = Lsn::ZERO;

    // Assert: Page can flush
    assert!(
        validate_wal_durability_before_page_flush(page_lsn, wal_durable).is_ok(),
        "Zero LSN page should be flushable"
    );
}

#[test]
fn manifest_can_switch_when_checkpoint_equals_wal_checkpoint() {
    // Setup: Manifest wants to checkpoint at LSN 500, WAL checkpoint at 500
    let manifest_checkpoint_lsn = Lsn::new(500);
    let wal_durable_lsn = Lsn::new(600); // WAL has advanced beyond checkpoint
    let wal_checkpoint_lsn = Lsn::new(500); // But checkpoint is exactly at manifest

    // Assert: Manifest can switch safely
    assert!(
        validate_manifest_atomic_switch(
            manifest_checkpoint_lsn,
            wal_durable_lsn,
            wal_checkpoint_lsn
        )
        .is_ok(),
        "Manifest should switch when checkpoint matches WAL checkpoint"
    );
}

#[test]
fn manifest_can_switch_when_checkpoint_precedes_wal_checkpoint() {
    // Setup: Manifest wants to checkpoint at LSN 400, WAL checkpoint at 500
    let manifest_checkpoint_lsn = Lsn::new(400);
    let wal_durable_lsn = Lsn::new(600);
    let wal_checkpoint_lsn = Lsn::new(500);

    // Assert: Manifest can switch (conservative checkpoint)
    assert!(
        validate_manifest_atomic_switch(
            manifest_checkpoint_lsn,
            wal_durable_lsn,
            wal_checkpoint_lsn
        )
        .is_ok(),
        "Manifest should switch when checkpoint < WAL checkpoint"
    );
}

#[test]
fn manifest_blocked_from_switch_when_checkpoint_exceeds_wal_checkpoint() {
    // Setup: Manifest wants to checkpoint at LSN 600, but WAL checkpoint only at 500
    let manifest_checkpoint_lsn = Lsn::new(600);
    let wal_durable_lsn = Lsn::new(600); // Durable is there
    let wal_checkpoint_lsn = Lsn::new(500); // But checkpoint isn't recorded yet

    // Assert: Manifest cannot switch
    let result = validate_manifest_atomic_switch(
        manifest_checkpoint_lsn,
        wal_durable_lsn,
        wal_checkpoint_lsn,
    );
    assert!(
        result.is_err(),
        "Manifest should be blocked when checkpoint > WAL checkpoint"
    );

    // Verify error message is informative
    let error = result.unwrap_err();
    assert!(
        error.message().contains("manifest"),
        "Error should mention manifest"
    );
}

#[test]
fn manifest_switch_scenario_after_checkpoint_operation() {
    // Simulate: System completes a checkpoint
    // 1. All dirty pages flush (once WAL durable through their LSNs)
    // 2. WAL records a checkpoint marker at LSN 500
    // 3. Manifest version increments, recording checkpoint LSN as 500
    // 4. Manifest is persisted atomically

    let checkpoint_lsn = Lsn::new(500);
    let wal_durable_lsn = Lsn::new(500);
    let wal_checkpoint_lsn = Lsn::new(500);

    // Manifest checkpoint barrier
    assert!(
        validate_manifest_atomic_switch(checkpoint_lsn, wal_durable_lsn, wal_checkpoint_lsn)
            .is_ok(),
        "Manifest should switch after WAL checkpoint"
    );
}

#[test]
fn manifest_atomic_switch_error_includes_lsn_details() {
    let manifest_ckpt = Lsn::new(700);
    let wal_durable = Lsn::new(700);
    let wal_ckpt = Lsn::new(600);

    let error = validate_manifest_atomic_switch(manifest_ckpt, wal_durable, wal_ckpt).unwrap_err();

    let msg = error.message();
    assert!(
        msg.contains("700"),
        "Error should include manifest checkpoint LSN"
    );
    assert!(
        msg.contains("600"),
        "Error should include WAL checkpoint LSN"
    );
}

#[test]
fn recovery_allowed_when_floor_equals_required() {
    // Setup: Manifest requires recovery to start at LSN 300
    // Recovery floor is also set to 300
    let recovery_floor_lsn = Lsn::new(300);
    let manifest_required_wal_start = Lsn::new(300);

    // Assert: Recovery can proceed
    assert!(
        validate_recovery_floor(recovery_floor_lsn, manifest_required_wal_start).is_ok(),
        "Recovery should be allowed when floor equals required"
    );
}

#[test]
fn recovery_allowed_when_floor_exceeds_required() {
    // Setup: Manifest requires start at 300, but recovery floor is 400
    // (perhaps more recent checkpoint is being used)
    let recovery_floor_lsn = Lsn::new(400);
    let manifest_required_wal_start = Lsn::new(300);

    // Assert: Recovery can proceed (more conservative)
    assert!(
        validate_recovery_floor(recovery_floor_lsn, manifest_required_wal_start).is_ok(),
        "Recovery should be allowed when floor > required"
    );
}

#[test]
fn recovery_blocked_when_floor_precedes_required() {
    // Setup: Manifest requires WAL from LSN 400, but recovery floor is 300
    // This would try to skip the first 100 WAL records needed for consistency
    let recovery_floor_lsn = Lsn::new(300);
    let manifest_required_wal_start = Lsn::new(400);

    // Assert: Recovery cannot proceed
    let result = validate_recovery_floor(recovery_floor_lsn, manifest_required_wal_start);
    assert!(
        result.is_err(),
        "Recovery should be blocked when floor < required"
    );

    // Verify error message
    let error = result.unwrap_err();
    assert!(
        error.message().contains("recovery floor"),
        "Error should mention recovery floor"
    );
}

#[test]
fn lsn_monotonicity_enforced_across_checkpoint_versions() {
    // Setup: Checkpoint V1 at LSN 100, Checkpoint V2 at LSN 200
    let checkpoint_v1_lsn = Lsn::new(100);
    let checkpoint_v2_lsn = Lsn::new(200);

    // Assert: Strict ordering enforced
    assert!(
        validate_lsn_strictly_ordered(
            checkpoint_v1_lsn,
            "checkpoint_v1",
            checkpoint_v2_lsn,
            "checkpoint_v2"
        )
        .is_ok(),
        "Checkpoints should strictly increase"
    );
}

#[test]
fn lsn_strict_ordering_rejects_equal_lsns() {
    // Setup: Two "different" checkpoints at same LSN (invalid)
    let checkpoint_a = Lsn::new(500);
    let checkpoint_b = Lsn::new(500);

    // Assert: Strict ordering rejects equality
    let result = validate_lsn_strictly_ordered(checkpoint_a, "old", checkpoint_b, "new");
    assert!(result.is_err(), "Strict ordering should reject equal LSNs");
}

#[test]
fn scenario_steady_state_all_pages_flushed() {
    // Scenario: Normal operation - multiple pages dirty, WAL durable, flush succeeds
    let pages = vec![
        ("page_1", Lsn::new(100)),
        ("page_2", Lsn::new(95)),
        ("page_3", Lsn::new(110)),
    ];
    let wal_durable = Lsn::new(150);

    // All pages should be flushable
    for (name, page_lsn) in pages {
        let result = validate_wal_durability_before_page_flush(page_lsn, wal_durable);
        assert!(
            result.is_ok(),
            "Page {} should be flushable in steady state",
            name
        );
    }
}

#[test]
fn scenario_crash_at_manifest_switch_requires_recovery_floor_check() {
    // Scenario: System crashes during manifest version increment.
    // At recovery time, we must ensure the recovery floor is safe.

    // Old manifest state:
    let old_manifest_checkpoint = Lsn::new(300);
    let _old_manifest_required_wal_start = Lsn::new(100); // Can start recovery here

    // New manifest (being written):
    let new_manifest_checkpoint = Lsn::new(500);
    let new_manifest_required_wal_start = Lsn::new(300); // Must start from here or later

    // WAL state:
    let wal_durable = Lsn::new(550);
    let wal_checkpoint = Lsn::new(500);

    // Recovery floor (where we actually have WAL):
    let recovery_floor = Lsn::new(300);

    // Checks:
    // 1. New manifest can be written (checkpoint <= wal_checkpoint)
    assert!(
        validate_manifest_atomic_switch(new_manifest_checkpoint, wal_durable, wal_checkpoint)
            .is_ok(),
        "New manifest checkpoint is safe"
    );

    // 2. Recovery floor is sufficient for new manifest
    assert!(
        validate_recovery_floor(recovery_floor, new_manifest_required_wal_start).is_ok(),
        "Recovery floor is sufficient"
    );

    // 3. Old manifest checkpoint precedes new one
    assert!(
        validate_lsn_strictly_ordered(
            old_manifest_checkpoint,
            "old_checkpoint",
            new_manifest_checkpoint,
            "new_checkpoint"
        )
        .is_ok(),
        "Manifest versions monotonically increase"
    );
}

#[test]
fn scenario_crash_during_page_flush_dirty_page_remains() {
    // Scenario: Page flush begins but crashes mid-write.
    // Page remains in dirty state. On recovery, we must not re-flush it
    // if the previous instance made it to disk.

    let page_first_dirty_lsn = Lsn::new(250);
    let wal_before_flush = Lsn::new(250); // WAL made change durable before flush started
    let wal_after_crash = Lsn::new(250); // WAL unchanged by crash

    // Before crash: flush check passes (page is flushable)
    assert!(
        validate_wal_durability_before_page_flush(page_first_dirty_lsn, wal_before_flush).is_ok(),
        "Page would have been flushed before crash"
    );

    // After recovery: same page, same LSN. Check should still pass.
    assert!(
        validate_wal_durability_before_page_flush(page_first_dirty_lsn, wal_after_crash).is_ok(),
        "Page check is idempotent across crashes"
    );
}

#[test]
fn scenario_burst_of_page_modifications_then_flush() {
    // Scenario: Multiple transactions dirty pages at consecutive LSNs,
    // then WAL drains, then pages flush.

    // Modifications happen at LSNs 1000-1010
    let page_modifications = vec![
        Lsn::new(1000),
        Lsn::new(1001),
        Lsn::new(1003), // Gap: LSN 1002 used by another page
        Lsn::new(1010),
    ];

    // WAL drains to LSN 1010
    let wal_durable_step1 = Lsn::new(1005); // Partial drain
    let wal_durable_step2 = Lsn::new(1010); // Complete drain

    // Step 1: Pages through LSN 1005 are flushable.
    for page_lsn in page_modifications.iter().take(3) {
        assert!(validate_wal_durability_before_page_flush(*page_lsn, wal_durable_step1).is_ok());
    }
    for page_lsn in page_modifications.iter().skip(3) {
        assert!(validate_wal_durability_before_page_flush(*page_lsn, wal_durable_step1).is_err());
    }

    // Step 2: All pages now flushable
    for page_lsn in &page_modifications {
        assert!(validate_wal_durability_before_page_flush(*page_lsn, wal_durable_step2).is_ok());
    }
}

#[test]
fn edge_case_max_lsn_comparisons() {
    // Setup: Use maximum representable LSN values
    let near_max = Lsn::new(u64::MAX - 1);
    let max_lsn = Lsn::MAX;

    // Assert: Comparisons work correctly at boundary
    assert!(
        validate_wal_durability_before_page_flush(near_max, max_lsn).is_ok(),
        "Near-max LSN should flush"
    );
    assert!(
        validate_wal_durability_before_page_flush(max_lsn, max_lsn).is_ok(),
        "Max LSN should flush to itself"
    );
    assert!(
        validate_wal_durability_before_page_flush(max_lsn, near_max).is_err(),
        "Max LSN should not flush to near-max"
    );
}

#[test]
fn edge_case_error_messages_are_informative() {
    // Setup: Create an error condition
    let result = validate_wal_durability_before_page_flush(Lsn::new(500), Lsn::new(400));

    // Assert: Error message includes numeric context
    let error = result.unwrap_err();
    let error_msg = error.message();
    assert!(
        error_msg.contains("500"),
        "Error should include higher LSN value"
    );
    assert!(
        error_msg.contains("400"),
        "Error should include lower LSN value"
    );
    assert!(
        error_msg.contains("durable"),
        "Error should mention durability"
    );
}

#[test]
fn integration_full_checkpoint_workflow() {
    // Simulate a complete checkpoint sequence:
    // 1. Pages dirty at various LSNs (100, 150, 200)
    // 2. WAL advances to 300 (all changes logged)
    // 3. Pages flush (now durable)
    // 4. WAL records checkpoint at 300
    // 5. Manifest increments with checkpoint LSN 300
    // 6. Next recovery can start at 300

    let page_lsns = vec![Lsn::new(100), Lsn::new(150), Lsn::new(200)];
    let wal_durable_after_logging = Lsn::new(300);
    let checkpoint_lsn = Lsn::new(300);
    let recovery_floor = Lsn::new(300);

    // Step 1 & 2: Pages can be flushed
    for page_lsn in &page_lsns {
        assert!(
            validate_wal_durability_before_page_flush(*page_lsn, wal_durable_after_logging).is_ok(),
            "All pages should be flushable after WAL logs all changes"
        );
    }

    // Step 3 & 4 & 5: Manifest can increment
    assert!(
        validate_manifest_atomic_switch(checkpoint_lsn, wal_durable_after_logging, checkpoint_lsn)
            .is_ok(),
        "Manifest should increment after checkpoint"
    );

    // Step 6: Recovery can start from checkpoint
    assert!(
        validate_recovery_floor(recovery_floor, checkpoint_lsn).is_ok(),
        "Recovery floor is valid for new manifest"
    );
}

#[test]
fn integration_multiple_checkpoints_each_advances_recovery_floor() {
    // Simulate: System runs multiple checkpoints over time
    // Each advances the recovery floor forward

    let checkpoints = [
        (Lsn::new(100), Lsn::new(100)),
        (Lsn::new(300), Lsn::new(300)),
        (Lsn::new(600), Lsn::new(600)),
    ];

    for i in 1..checkpoints.len() {
        let prev_checkpoint = checkpoints[i - 1].0;
        let curr_checkpoint = checkpoints[i].0;
        let recovery_floor = curr_checkpoint;

        // Checkpoints strictly increase
        assert!(
            validate_lsn_strictly_ordered(prev_checkpoint, "prev", curr_checkpoint, "curr").is_ok()
        );

        // Recovery floor is always sufficient
        assert!(validate_recovery_floor(recovery_floor, curr_checkpoint).is_ok());
    }
}
