use super::support::*;
use andromeda_tx::{ReclamationEligibility, ReclamationMark};

#[test]
fn test_eligibility_criteria_combinations() {
    let eligibility_scenarios = [
        (true, true, true, true),
        (false, true, true, false),
        (true, false, true, false),
        (true, true, false, false),
        (false, false, false, false),
    ];

    for (committed, invisible, grace_ok, expected_eligible) in eligibility_scenarios {
        let eligibility = ReclamationEligibility::new(committed, invisible, grace_ok);
        assert_eq!(
            eligibility.is_fully_eligible(),
            expected_eligible,
            "Eligibility should be: committed={}, invisible={}, grace_ok={}",
            committed,
            invisible,
            grace_ok
        );
    }
}

#[test]
fn test_mark_runtime_eligibility_check() {
    let (status_table, tx_id) = committed_creator(1);
    let mark = ReclamationMark::new(
        1,
        tx_id,
        DEFAULT_END_TS,
        DEFAULT_MARKED_AT,
        DEFAULT_GC_EPOCH,
    )
    .unwrap();

    assert!(mark.is_eligible(
        &status_table,
        DEFAULT_MIN_VISIBLE_TS,
        0,
        DEFAULT_CURRENT_GC_EPOCH
    ));

    let eligibility = mark.check_eligibility(
        &status_table,
        DEFAULT_MIN_VISIBLE_TS,
        0,
        DEFAULT_CURRENT_GC_EPOCH,
    );
    assert!(eligibility.is_fully_eligible());

    let regressed_frontier = mark.check_eligibility(&status_table, 40, 0, DEFAULT_CURRENT_GC_EPOCH);
    assert!(!regressed_frontier.is_end_ts_invisible);
    assert!(!regressed_frontier.is_fully_eligible());
}

#[test]
fn test_grace_period_multiple_epochs() {
    let (status_table, tx_id) = committed_creator(1);
    let grace_period_epochs = 10;
    let mark = ReclamationMark::new(1, tx_id, DEFAULT_END_TS, DEFAULT_MARKED_AT, 5).unwrap();

    for current_epoch in [10, 14] {
        let eligibility = mark.check_eligibility(
            &status_table,
            DEFAULT_MIN_VISIBLE_TS,
            grace_period_epochs,
            current_epoch,
        );
        assert!(!eligibility.gc_epoch_qualified);
    }

    for current_epoch in [15, 100] {
        let eligibility = mark.check_eligibility(
            &status_table,
            DEFAULT_MIN_VISIBLE_TS,
            grace_period_epochs,
            current_epoch,
        );
        assert!(eligibility.gc_epoch_qualified);
    }
}
