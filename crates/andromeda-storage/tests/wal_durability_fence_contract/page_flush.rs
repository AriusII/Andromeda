use crate::support::{
    assert_message_contains, assert_page_flush_allowed, assert_page_flush_blocked,
};
use andromeda_core::AndromedaErrorKind;
use andromeda_wal::{Lsn, validate_wal_durability_before_page_flush};

#[test]
fn page_can_flush_when_first_dirty_lsn_equals_wal_durable_lsn() {
    assert_page_flush_allowed(
        Lsn::new(100),
        Lsn::new(100),
        "Page should be flushable when first_dirty_lsn equals wal_durable_lsn",
    );
}

#[test]
fn page_can_flush_when_first_dirty_lsn_precedes_wal_durable_lsn() {
    assert_page_flush_allowed(
        Lsn::new(50),
        Lsn::new(100),
        "Page should be flushable when first_dirty_lsn < wal_durable_lsn",
    );
}

#[test]
fn page_blocked_from_flush_when_first_dirty_lsn_exceeds_wal_durable_lsn() {
    let result = validate_wal_durability_before_page_flush(Lsn::new(150), Lsn::new(100));
    assert!(
        result.is_err(),
        "Page should be blocked when first_dirty_lsn > wal_durable_lsn"
    );

    let error = result.unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert_message_contains(
        error.message(),
        "150",
        "Error message should include page LSN",
    );
    assert_message_contains(
        error.message(),
        "100",
        "Error message should include WAL LSN",
    );
}

#[test]
fn page_multiple_dirty_cycles_require_latest_dirty_lsn() {
    assert_page_flush_allowed(
        Lsn::new(250),
        Lsn::new(250),
        "Page should be flushable once WAL durable through latest modification",
    );
}

#[test]
fn page_multiple_dirty_cycles_cannot_flush_on_first_dirty_lsn_only() {
    assert_page_flush_blocked(
        Lsn::new(250),
        Lsn::new(200),
        "Page must remain blocked until durable WAL reaches the latest dirty LSN",
    );
}

#[test]
fn page_zero_lsn_special_case() {
    assert_page_flush_allowed(Lsn::ZERO, Lsn::ZERO, "Zero LSN page should be flushable");
}

#[test]
fn steady_state_pages_flush_after_wal_is_durable() {
    let pages = [
        ("page_1", Lsn::new(100)),
        ("page_2", Lsn::new(95)),
        ("page_3", Lsn::new(110)),
    ];
    let wal_durable = Lsn::new(150);

    for (name, page_lsn) in pages {
        assert_page_flush_allowed(
            page_lsn,
            wal_durable,
            &format!("Page {name} should be flushable in steady state"),
        );
    }
}

#[test]
fn crash_during_page_flush_preserves_wal_fence_idempotence() {
    let page_first_dirty_lsn = Lsn::new(250);
    let wal_before_flush = Lsn::new(250);
    let wal_after_crash = Lsn::new(250);

    assert_page_flush_allowed(
        page_first_dirty_lsn,
        wal_before_flush,
        "Page would have been flushed before crash",
    );
    assert_page_flush_allowed(
        page_first_dirty_lsn,
        wal_after_crash,
        "Page check is idempotent across crashes",
    );
}

#[test]
fn burst_page_modifications_flush_only_after_wal_catches_up() {
    let page_modifications = [
        Lsn::new(1000),
        Lsn::new(1001),
        Lsn::new(1003),
        Lsn::new(1010),
    ];

    let wal_durable_step1 = Lsn::new(1005);
    let wal_durable_step2 = Lsn::new(1010);

    for page_lsn in page_modifications.iter().take(3) {
        assert_page_flush_allowed(
            *page_lsn,
            wal_durable_step1,
            "Page should flush after partial WAL drain covers its LSN",
        );
    }
    for page_lsn in page_modifications.iter().skip(3) {
        assert_page_flush_blocked(
            *page_lsn,
            wal_durable_step1,
            "Page should remain blocked until WAL reaches its LSN",
        );
    }

    for page_lsn in &page_modifications {
        assert_page_flush_allowed(
            *page_lsn,
            wal_durable_step2,
            "Page should flush after complete WAL drain",
        );
    }
}

#[test]
fn max_lsn_comparisons_preserve_wal_fence() {
    let near_max = Lsn::new(u64::MAX - 1);
    let max_lsn = Lsn::MAX;

    assert_page_flush_allowed(near_max, max_lsn, "Near-max LSN should flush");
    assert_page_flush_allowed(max_lsn, max_lsn, "Max LSN should flush to itself");
    assert_page_flush_blocked(max_lsn, near_max, "Max LSN should not flush to near-max");
}

#[test]
fn wal_fence_errors_include_lsn_context() {
    let result = validate_wal_durability_before_page_flush(Lsn::new(500), Lsn::new(400));
    let error = result.unwrap_err();
    let error_msg = error.message();

    assert_message_contains(error_msg, "500", "Error should include higher LSN value");
    assert_message_contains(error_msg, "400", "Error should include lower LSN value");
    assert_message_contains(error_msg, "durable", "Error should mention durability");
}
