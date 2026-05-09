use super::support::*;
use andromeda_mvcc::{ReclamationMark, TransactionStatusTable};

#[test]
fn test_mark_created_for_eligible_version() {
    let (status_table, tx_id) = committed_creator(1);

    let mark_opt = ReclamationMark::from_version_if_eligible(
        default_candidate(1, tx_id),
        &status_table,
        DEFAULT_MIN_VISIBLE_TS,
        0,
        DEFAULT_CURRENT_GC_EPOCH,
    );

    assert!(mark_opt.is_ok(), "Should create mark without error");
    assert!(
        mark_opt.unwrap().is_some(),
        "Should return Some when fully eligible"
    );
}

#[test]
fn test_mark_rejected_uncommitted_creator() {
    let status_table = TransactionStatusTable::new();
    let tx_id = tx_id(1);

    let mark_opt = ReclamationMark::from_version_if_eligible(
        default_candidate(1, tx_id),
        &status_table,
        DEFAULT_MIN_VISIBLE_TS,
        0,
        DEFAULT_CURRENT_GC_EPOCH,
    );

    assert!(mark_opt.is_ok());
    assert!(
        mark_opt.unwrap().is_none(),
        "Should not create mark for uncommitted creator"
    );
}

#[test]
fn test_mark_rejected_version_still_visible() {
    let (status_table, tx_id) = committed_creator(1);

    let mark_opt = ReclamationMark::from_version_if_eligible(
        candidate(1, tx_id, 150, DEFAULT_MARKED_AT, DEFAULT_GC_EPOCH),
        &status_table,
        DEFAULT_MIN_VISIBLE_TS,
        0,
        DEFAULT_CURRENT_GC_EPOCH,
    );

    assert!(mark_opt.is_ok());
    assert!(
        mark_opt.unwrap().is_none(),
        "Should not mark version still visible to active snapshots"
    );
}

#[test]
fn test_mark_rejected_grace_period_not_expired() {
    let (status_table, tx_id) = committed_creator(1);

    let mark_opt = ReclamationMark::from_version_if_eligible(
        candidate(1, tx_id, DEFAULT_END_TS, DEFAULT_MARKED_AT, 10),
        &status_table,
        DEFAULT_MIN_VISIBLE_TS,
        5,
        DEFAULT_CURRENT_GC_EPOCH,
    );

    assert!(mark_opt.is_ok());
    assert!(
        mark_opt.unwrap().is_none(),
        "Should reject mark when grace period not expired"
    );
}

#[test]
fn test_mark_emits_reclamation_command() {
    let mark = ReclamationMark::new(1, tx_id(1), 100, 50, 5).unwrap();

    let cmd = mark.mark_for_reclamation();

    assert_eq!(cmd.version_id, 1);
    assert_eq!(cmd.creator_tx_id, tx_id(1));
    assert_eq!(cmd.end_ts, 100);
}

#[test]
fn test_mark_validation_rejects_invalid_inputs() {
    assert!(ReclamationMark::new(0, tx_id(1), 100, 50, 5).is_err());
    assert!(ReclamationMark::new(1, tx_id(0), 100, 50, 5).is_err());
    assert!(ReclamationMark::new(1, tx_id(1), u64::MAX, 50, 5).is_err());
    assert!(ReclamationMark::new(1, tx_id(1), 100, 50, 5).is_ok());
}

#[test]
fn test_reclamation_command_generation() {
    let (status_table, tx_id) = committed_creator(1);

    let mark = ReclamationMark::from_version_if_eligible(
        candidate(42, tx_id, 500, 100, 10),
        &status_table,
        1000,
        0,
        20,
    )
    .unwrap()
    .unwrap();

    let cmd = mark.mark_for_reclamation();

    assert_eq!(cmd.version_id, 42);
    assert_eq!(cmd.creator_tx_id, tx_id);
    assert_eq!(cmd.end_ts, 500);
}
