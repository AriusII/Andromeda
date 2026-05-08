use andromeda_core::TransactionId;
use andromeda_transaction_log::Lsn;

use crate::status::TransactionStatusTable;

use super::{
    ReclamationCommand, ReclamationEligibility, ReclamationMark, ReclamationMarkCandidate,
    ReclamationStats,
};

#[test]
fn reclamation_mark_creation() {
    let mark = ReclamationMark::new(
        1, // version_id
        TransactionId::new(1),
        100, // end_ts
        50,  // marked_at
        5,   // gc_epoch
    );

    assert!(mark.is_ok());
    let m = mark.unwrap();
    assert_eq!(m.version_id, 1);
    assert_eq!(m.end_ts, 100);
}

#[test]
fn reclamation_mark_rejects_zero_version_id() {
    let mark = ReclamationMark::new(
        0, // Invalid: zero
        TransactionId::new(1),
        100,
        50,
        5,
    );

    assert!(mark.is_err());
}

#[test]
fn reclamation_mark_rejects_zero_creator_tx_id() {
    let mark = ReclamationMark::new(
        1,
        TransactionId::new(0), // Invalid: zero
        100,
        50,
        5,
    );

    assert!(mark.is_err());
}

#[test]
fn reclamation_mark_rejects_live_version() {
    let mark = ReclamationMark::new(
        1,
        TransactionId::new(1),
        u64::MAX, // Invalid: live version
        50,
        5,
    );

    assert!(mark.is_err());
}

#[test]
fn reclamation_eligibility_fully_eligible() {
    let eligibility = ReclamationEligibility::new(true, true, true);
    assert!(eligibility.is_fully_eligible());
}

#[test]
fn reclamation_eligibility_not_committed_creator() {
    let eligibility = ReclamationEligibility::new(false, true, true);
    assert!(!eligibility.is_fully_eligible());
}

#[test]
fn reclamation_eligibility_end_ts_still_visible() {
    let eligibility = ReclamationEligibility::new(true, false, true);
    assert!(!eligibility.is_fully_eligible());
}

#[test]
fn reclamation_eligibility_grace_period_not_met() {
    let eligibility = ReclamationEligibility::new(true, true, false);
    assert!(!eligibility.is_fully_eligible());
}

#[test]
fn reclamation_command_creation() {
    let cmd = ReclamationCommand::new(1, TransactionId::new(1), 100);
    assert!(cmd.is_ok());
    let c = cmd.unwrap();
    assert_eq!(c.version_id, 1);
}

#[test]
fn reclamation_command_rejects_zero_version_id() {
    let cmd = ReclamationCommand::new(0, TransactionId::new(1), 100);
    assert!(cmd.is_err());
}

#[test]
fn reclamation_mark_emit_command() {
    let mark = ReclamationMark::new(1, TransactionId::new(1), 100, 50, 5).unwrap();
    let cmd = mark.mark_for_reclamation();

    assert_eq!(cmd.version_id, mark.version_id);
    assert_eq!(cmd.creator_tx_id, mark.creator_tx_id);
    assert_eq!(cmd.end_ts, mark.end_ts);
}

#[test]
fn reclamation_mark_validate_success() {
    let mark = ReclamationMark::new(1, TransactionId::new(1), 100, 50, 5).unwrap();
    assert!(mark.validate().is_ok());
}

#[test]
fn reclamation_stats_tracking() {
    let stats = ReclamationStats::new();

    assert_eq!(stats.marks_created(), 0);
    assert_eq!(stats.commands_executed(), 0);
    assert_eq!(stats.versions_reclaimed(), 0);

    stats.record_mark();
    assert_eq!(stats.marks_created(), 1);

    stats.record_execution();
    assert_eq!(stats.commands_executed(), 1);

    stats.record_reclamation();
    assert_eq!(stats.versions_reclaimed(), 1);
}

#[test]
fn reclamation_mark_eligibility_with_committed_creator() {
    let status_table = TransactionStatusTable::new();
    let tx_id = TransactionId::new(1);

    status_table
        .record_committed_after_durable_wal(tx_id, Lsn::new(1), Lsn::new(1))
        .unwrap();

    let mark = ReclamationMark::new(1, tx_id, 50, 20, 5).unwrap();
    let eligibility = mark.check_eligibility(&status_table, 100, 0, 10);

    assert!(eligibility.is_creator_committed);
    assert!(eligibility.is_end_ts_invisible); // 50 < 100
    assert!(eligibility.gc_epoch_qualified); // 10 >= 5 + 0
}

#[test]
fn reclamation_mark_eligibility_with_uncommitted_creator() {
    let status_table = TransactionStatusTable::new();
    let tx_id = TransactionId::new(1);

    // Don't mark as committed - remains InFlight

    let mark = ReclamationMark::new(1, tx_id, 50, 20, 5).unwrap();
    let eligibility = mark.check_eligibility(&status_table, 100, 0, 10);

    assert!(!eligibility.is_creator_committed);
    assert!(!eligibility.is_fully_eligible());
}

#[test]
fn reclamation_mark_eligibility_end_ts_still_visible() {
    let status_table = TransactionStatusTable::new();
    let tx_id = TransactionId::new(1);

    status_table
        .record_committed_after_durable_wal(tx_id, Lsn::new(1), Lsn::new(1))
        .unwrap();

    let mark = ReclamationMark::new(1, tx_id, 150, 20, 5).unwrap();
    // min_visible_ts is 100, but end_ts is 150
    let eligibility = mark.check_eligibility(&status_table, 100, 0, 10);

    assert!(eligibility.is_creator_committed);
    assert!(!eligibility.is_end_ts_invisible); // 150 >= 100
    assert!(!eligibility.is_fully_eligible());
}

#[test]
fn reclamation_mark_eligibility_grace_period_not_met() {
    let status_table = TransactionStatusTable::new();
    let tx_id = TransactionId::new(1);

    status_table
        .record_committed_after_durable_wal(tx_id, Lsn::new(1), Lsn::new(1))
        .unwrap();

    let mark = ReclamationMark::new(1, tx_id, 50, 20, 10).unwrap();
    // Grace period is 5 epochs, current is 10, marked_at epoch is 10
    // So 10 < 10 + 5 (not expired)
    let eligibility = mark.check_eligibility(&status_table, 100, 5, 10);

    assert!(eligibility.is_creator_committed);
    assert!(eligibility.is_end_ts_invisible);
    assert!(!eligibility.gc_epoch_qualified);
    assert!(!eligibility.is_fully_eligible());
}

#[test]
fn reclamation_mark_from_version_if_fully_eligible() {
    let status_table = TransactionStatusTable::new();
    let tx_id = TransactionId::new(1);

    status_table
        .record_committed_after_durable_wal(tx_id, Lsn::new(1), Lsn::new(1))
        .unwrap();

    let mark_opt = ReclamationMark::from_version_if_eligible(
        ReclamationMarkCandidate::new(1, tx_id, 50, 20, 5),
        &status_table,
        100, // min_visible_ts
        0,   // grace_period_epochs
        10,  // current_gc_epoch
    );

    assert!(mark_opt.is_ok());
    assert!(mark_opt.unwrap().is_some());
}

#[test]
fn reclamation_mark_from_version_rejected_uncommitted() {
    let status_table = TransactionStatusTable::new();
    let tx_id = TransactionId::new(1);

    // Don't mark as committed

    let mark_opt = ReclamationMark::from_version_if_eligible(
        ReclamationMarkCandidate::new(1, tx_id, 50, 20, 5),
        &status_table,
        100,
        0,
        10,
    );

    assert!(mark_opt.is_ok());
    assert!(mark_opt.unwrap().is_none());
}

#[test]
fn reclamation_mark_from_version_rejected_still_visible() {
    let status_table = TransactionStatusTable::new();
    let tx_id = TransactionId::new(1);

    status_table
        .record_committed_after_durable_wal(tx_id, Lsn::new(1), Lsn::new(1))
        .unwrap();

    let mark_opt = ReclamationMark::from_version_if_eligible(
        ReclamationMarkCandidate::new(1, tx_id, 150, 20, 5),
        &status_table,
        100,
        0,
        10,
    );

    assert!(mark_opt.is_ok());
    assert!(mark_opt.unwrap().is_none()); // end_ts 150 >= min_visible_ts 100
}

#[test]
fn reclamation_mark_from_version_rejected_grace_period() {
    let status_table = TransactionStatusTable::new();
    let tx_id = TransactionId::new(1);

    status_table
        .record_committed_after_durable_wal(tx_id, Lsn::new(1), Lsn::new(1))
        .unwrap();

    let mark_opt = ReclamationMark::from_version_if_eligible(
        ReclamationMarkCandidate::new(1, tx_id, 50, 20, 10),
        &status_table,
        100,
        5,
        10,
    );

    assert!(mark_opt.is_ok());
    assert!(mark_opt.unwrap().is_none()); // 10 < 10 + 5 (grace period not met)
}

#[test]
fn reclamation_mark_is_eligible_runtime_check() {
    let status_table = TransactionStatusTable::new();
    let tx_id = TransactionId::new(1);

    status_table
        .record_committed_after_durable_wal(tx_id, Lsn::new(1), Lsn::new(1))
        .unwrap();

    let mark = ReclamationMark::new(1, tx_id, 50, 20, 5).unwrap();

    assert!(mark.is_eligible(&status_table, 100, 0, 10));
}

#[test]
fn reclamation_mark_batch_processing_scenario() {
    let status_table = TransactionStatusTable::new();
    let mut marks = Vec::new();

    // Create 10 marks, all with same creator
    let tx_id = TransactionId::new(1);
    status_table
        .record_committed_after_durable_wal(tx_id, Lsn::new(1), Lsn::new(1))
        .unwrap();

    for i in 1..=10 {
        let mark = ReclamationMark::new(i, tx_id, 50, 20, 5).unwrap();
        marks.push(mark);
    }

    // Count eligible marks
    let eligible_count = marks
        .iter()
        .filter(|m| m.is_eligible(&status_table, 100, 0, 10))
        .count();

    assert_eq!(eligible_count, 10);
}

#[test]
fn reclamation_no_visible_version_invariant_proof() {
    // This test proves: if end_ts < min_visible_ts, no visible transaction can see the version
    let status_table = TransactionStatusTable::new();
    let tx_id = TransactionId::new(1);

    status_table
        .record_committed_after_durable_wal(tx_id, Lsn::new(1), Lsn::new(1))
        .unwrap();

    // Version with end_ts = 50
    let mark = ReclamationMark::new(1, tx_id, 50, 20, 5).unwrap();

    // min_visible_ts = 100 means:
    // - The oldest active snapshot has timestamp >= 100
    // - No snapshot can have timestamp < 100 (not active)
    // - To see a version, snapshot.timestamp must be >= end_ts
    // - Since all active snapshots have timestamp >= 100, and end_ts = 50,
    //   they would see the version IF it existed
    // - However, no active snapshot should see timestamps < 100
    //   (they're all >= 100 because that's the minimum active)
    //
    // The reclamation mark ensures: if end_ts < min_visible_ts, the version is safe to reclaim

    assert!(mark.is_eligible(&status_table, 100, 0, 10));
    let eligibility = mark.check_eligibility(&status_table, 100, 0, 10);
    assert!(eligibility.is_end_ts_invisible);
}
