use super::support::*;
use andromeda_mvcc::ReclamationMark;

#[test]
fn test_no_visible_version_can_be_reclaimed() {
    let (status_table, creator_tx_id) = committed_creator(1);

    let boundary_mark = ReclamationMark::from_version_if_eligible(
        candidate(1, creator_tx_id, 100, DEFAULT_MARKED_AT, DEFAULT_GC_EPOCH),
        &status_table,
        100,
        0,
        DEFAULT_CURRENT_GC_EPOCH,
    );

    assert!(boundary_mark.is_ok());
    assert!(
        boundary_mark.unwrap().is_none(),
        "Version at boundary should not be reclaimed"
    );

    let advanced_frontier = ReclamationMark::from_version_if_eligible(
        candidate(1, creator_tx_id, 100, DEFAULT_MARKED_AT, DEFAULT_GC_EPOCH),
        &status_table,
        101,
        0,
        DEFAULT_CURRENT_GC_EPOCH,
    );

    assert!(advanced_frontier.is_ok());
    assert!(
        advanced_frontier.unwrap().is_some(),
        "Version becomes reclaimable when end_ts < min_visible_ts"
    );
}

#[test]
fn test_version_at_exact_min_visible_ts_boundary() {
    let (status_table, tx_id) = committed_creator(1);

    let mark_opt = ReclamationMark::from_version_if_eligible(
        candidate(
            1,
            tx_id,
            DEFAULT_MIN_VISIBLE_TS,
            DEFAULT_MARKED_AT,
            DEFAULT_GC_EPOCH,
        ),
        &status_table,
        DEFAULT_MIN_VISIBLE_TS,
        0,
        DEFAULT_CURRENT_GC_EPOCH,
    );

    assert!(mark_opt.is_ok());
    assert!(
        mark_opt.unwrap().is_none(),
        "Version at boundary should not be marked"
    );
}
