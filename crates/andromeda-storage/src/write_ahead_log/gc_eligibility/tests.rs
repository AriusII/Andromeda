use super::*;
use crate::Lsn;
use andromeda_core::AndromedaErrorKind;

#[test]
fn test_new_rejects_zero_start_lsn() {
    let result = GcEligibilityChecker::new(Lsn::new(0), Lsn::new(100), Lsn::new(200), Lsn::new(50));
    assert!(result.is_err());
}

#[test]
fn test_new_rejects_zero_end_lsn() {
    let result = GcEligibilityChecker::new(Lsn::new(1), Lsn::new(0), Lsn::new(200), Lsn::new(50));
    assert!(result.is_err());
}

#[test]
fn test_new_rejects_start_exceeds_end() {
    let result =
        GcEligibilityChecker::new(Lsn::new(100), Lsn::new(50), Lsn::new(200), Lsn::new(25));
    assert!(result.is_err());
}

#[test]
fn test_new_rejects_recovery_exceeds_snapshot() {
    let result =
        GcEligibilityChecker::new(Lsn::new(1), Lsn::new(100), Lsn::new(150), Lsn::new(200));
    assert!(result.is_err());
}

#[test]
fn test_eligible_segment_in_safe_zone() {
    let checker =
        GcEligibilityChecker::new(Lsn::new(101), Lsn::new(199), Lsn::new(200), Lsn::new(100))
            .unwrap();

    assert_eq!(checker.is_eligible(), EligibilityResult::Eligible);
}

#[test]
fn test_visibility_boundary_is_strictly_after_segment_end() {
    let checker =
        GcEligibilityChecker::new(Lsn::new(101), Lsn::new(199), Lsn::new(200), Lsn::new(100))
            .unwrap();

    // The active snapshot boundary is just past the segment end, so the
    // segment is wholly older than every active snapshot.
    assert_eq!(checker.is_eligible(), EligibilityResult::Eligible);
}

#[test]
fn test_blocked_by_visibility_end_at_boundary() {
    let checker =
        GcEligibilityChecker::new(Lsn::new(150), Lsn::new(200), Lsn::new(200), Lsn::new(100))
            .unwrap();

    let result = checker.is_eligible();
    assert!(matches!(
        result,
        EligibilityResult::BlockedByVisibility { .. }
    ));
}

#[test]
fn test_blocked_by_visibility_when_segment_is_newer_than_snapshot() {
    let checker =
        GcEligibilityChecker::new(Lsn::new(201), Lsn::new(250), Lsn::new(200), Lsn::new(100))
            .unwrap();

    // A segment that begins after the oldest active snapshot is newer than
    // that snapshot's visibility boundary and must not be reclaimed.
    assert_eq!(
        checker.is_eligible(),
        EligibilityResult::BlockedByVisibility {
            segment_end_lsn: Lsn::new(250),
            min_active_snapshot_lsn: Lsn::new(200),
        }
    );
}

#[test]
fn test_blocked_by_visibility_segment_overlaps_snapshot() {
    let checker =
        GcEligibilityChecker::new(Lsn::new(150), Lsn::new(250), Lsn::new(200), Lsn::new(50))
            .unwrap();

    assert!(matches!(
        checker.is_eligible(),
        EligibilityResult::BlockedByVisibility { .. }
    ));
}

#[test]
fn test_blocked_by_recovery_start_at_recovery_boundary() {
    let checker =
        GcEligibilityChecker::new(Lsn::new(100), Lsn::new(199), Lsn::new(200), Lsn::new(100))
            .unwrap();

    assert!(matches!(
        checker.is_eligible(),
        EligibilityResult::BlockedByRecovery { .. }
    ));
}

#[test]
fn test_blocked_by_recovery_segment_before_recovery() {
    let checker =
        GcEligibilityChecker::new(Lsn::new(50), Lsn::new(99), Lsn::new(300), Lsn::new(100))
            .unwrap();

    assert!(matches!(
        checker.is_eligible(),
        EligibilityResult::BlockedByRecovery { .. }
    ));
}

#[test]
fn test_both_blocked_by_visibility_first() {
    // Blocked by both rules, but visibility check comes first
    let checker =
        GcEligibilityChecker::new(Lsn::new(100), Lsn::new(200), Lsn::new(200), Lsn::new(100))
            .unwrap();

    // The segment contains min_snapshot (200), and segment_start (100) <= recovery (100),
    // so both predicates block. Visibility is reported first for deterministic observability.
    let result = checker.is_eligible();
    assert!(matches!(
        result,
        EligibilityResult::BlockedByVisibility { .. }
    ));
}

#[test]
fn test_validate_passes_for_valid_checker() {
    let checker =
        GcEligibilityChecker::new(Lsn::new(101), Lsn::new(199), Lsn::new(200), Lsn::new(100))
            .unwrap();

    assert!(checker.validate().is_ok());
}

#[test]
fn test_lsn_distance_zero_when_eligible() {
    let checker =
        GcEligibilityChecker::new(Lsn::new(101), Lsn::new(199), Lsn::new(200), Lsn::new(100))
            .unwrap();

    assert_eq!(checker.lsn_distance_to_eligibility(), 0);
}

#[test]
fn test_lsn_distance_zero_when_segment_before_snapshot_boundary() {
    let checker =
        GcEligibilityChecker::new(Lsn::new(150), Lsn::new(180), Lsn::new(200), Lsn::new(50))
            .unwrap();

    let distance = checker.lsn_distance_to_eligibility();
    // Segment is wholly before the active snapshot boundary and is not blocked by recovery.
    assert_eq!(distance, 0);
}

#[test]
fn test_lsn_distance_blocked_by_visibility_correct() {
    let checker =
        GcEligibilityChecker::new(Lsn::new(150), Lsn::new(205), Lsn::new(200), Lsn::new(50))
            .unwrap();

    // min_snapshot (200) is inside [150, 205], so visibility blocks until the
    // snapshot boundary advances past 205.
    assert_eq!(
        checker.is_eligible(),
        EligibilityResult::BlockedByVisibility {
            segment_end_lsn: Lsn::new(205),
            min_active_snapshot_lsn: Lsn::new(200),
        }
    );
    assert_eq!(checker.lsn_distance_to_eligibility(), 6);
}

#[test]
fn test_lsn_distance_when_segment_end_equals_snapshot() {
    let checker =
        GcEligibilityChecker::new(Lsn::new(150), Lsn::new(200), Lsn::new(200), Lsn::new(50))
            .unwrap();

    // min_snapshot (200) is the segment end boundary, so the segment overlaps visibility.
    assert!(matches!(
        checker.is_eligible(),
        EligibilityResult::BlockedByVisibility { .. }
    ));
}

#[test]
fn test_segment_accessors() {
    let checker =
        GcEligibilityChecker::new(Lsn::new(300), Lsn::new(399), Lsn::new(200), Lsn::new(100))
            .unwrap();

    assert_eq!(checker.segment_start(), Lsn::new(300));
    assert_eq!(checker.segment_end(), Lsn::new(399));
    assert_eq!(checker.min_active_snapshot_lsn(), Lsn::new(200));
    assert_eq!(checker.required_recovery_lsn(), Lsn::new(100));
}

#[test]
fn test_eligibility_result_reason() {
    assert_eq!(
        EligibilityResult::Eligible.reason(),
        "segment is older than the oldest active snapshot and is not needed for recovery"
    );
}

#[test]
fn test_multiple_segments_cascade() {
    // Simulate a cascade of segments with recovery boundary at 100,
    // min snapshot at 300
    let ineligible_recovery =
        GcEligibilityChecker::new(Lsn::new(1), Lsn::new(99), Lsn::new(300), Lsn::new(100)).unwrap();
    assert!(!ineligible_recovery.is_eligible().is_eligible());

    let boundary_recovery =
        GcEligibilityChecker::new(Lsn::new(100), Lsn::new(199), Lsn::new(300), Lsn::new(100))
            .unwrap();
    assert!(!boundary_recovery.is_eligible().is_eligible());

    let ineligible_visibility =
        GcEligibilityChecker::new(Lsn::new(200), Lsn::new(300), Lsn::new(300), Lsn::new(100))
            .unwrap();
    assert!(!ineligible_visibility.is_eligible().is_eligible());

    let eligible =
        GcEligibilityChecker::new(Lsn::new(101), Lsn::new(199), Lsn::new(300), Lsn::new(100))
            .unwrap();
    assert!(eligible.is_eligible().is_eligible());
}

#[test]
fn test_error_kind_on_invalid_construction() {
    let result = GcEligibilityChecker::new(Lsn::new(0), Lsn::new(100), Lsn::new(200), Lsn::new(50));
    assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Storage);
}

#[test]
fn test_eligibility_result_is_eligible_method() {
    assert!(EligibilityResult::Eligible.is_eligible());
    assert!(
        !EligibilityResult::BlockedByVisibility {
            segment_end_lsn: Lsn::new(100),
            min_active_snapshot_lsn: Lsn::new(200),
        }
        .is_eligible()
    );
    assert!(
        !EligibilityResult::BlockedByRecovery {
            segment_start_lsn: Lsn::new(50),
            required_recovery_lsn: Lsn::new(100),
        }
        .is_eligible()
    );
}
