use super::support::*;
use andromeda_tx::{ReclamationMark, ReclamationStats, TransactionStatusTable};

#[test]
fn test_batch_reclamation_100_marks() {
    let (status_table, tx_id) = committed_creator(1);
    let mut marks = Vec::new();

    for version_id in 1..=100 {
        let mark_opt = ReclamationMark::from_version_if_eligible(
            candidate(
                version_id,
                tx_id,
                version_id * 10,
                DEFAULT_MARKED_AT,
                DEFAULT_GC_EPOCH,
            ),
            &status_table,
            2000,
            0,
            DEFAULT_CURRENT_GC_EPOCH,
        );

        assert!(mark_opt.is_ok());
        if let Ok(Some(mark)) = mark_opt {
            marks.push(mark);
        }
    }

    assert_eq!(marks.len(), 100, "Should create all 100 marks");

    let mut version_ids: Vec<u64> = marks.iter().map(|m| m.version_id).collect();
    version_ids.sort();
    version_ids.dedup();
    assert_eq!(
        version_ids.len(),
        100,
        "All marks should have distinct version_ids"
    );
}

#[test]
fn test_reclamation_multiple_creators() {
    let status_table = TransactionStatusTable::new();
    let tx1 = tx_id(1);
    let tx2 = tx_id(2);
    let tx3 = tx_id(3);

    record_committed(&status_table, tx1);
    record_committed(&status_table, tx3);

    let mark1 = ReclamationMark::from_version_if_eligible(
        default_candidate(1, tx1),
        &status_table,
        DEFAULT_MIN_VISIBLE_TS,
        0,
        DEFAULT_CURRENT_GC_EPOCH,
    );
    let mark2 = ReclamationMark::from_version_if_eligible(
        default_candidate(2, tx2),
        &status_table,
        DEFAULT_MIN_VISIBLE_TS,
        0,
        DEFAULT_CURRENT_GC_EPOCH,
    );
    let mark3 = ReclamationMark::from_version_if_eligible(
        default_candidate(3, tx3),
        &status_table,
        DEFAULT_MIN_VISIBLE_TS,
        0,
        DEFAULT_CURRENT_GC_EPOCH,
    );

    assert!(
        mark1.unwrap().is_some(),
        "tx1 mark should exist (committed)"
    );
    assert!(
        mark2.unwrap().is_none(),
        "tx2 mark should not exist (uncommitted)"
    );
    assert!(
        mark3.unwrap().is_some(),
        "tx3 mark should exist (committed)"
    );
}

#[test]
fn test_reclamation_stats_accumulated() {
    let stats = ReclamationStats::new();

    assert_eq!(stats.marks_created(), 0);
    assert_eq!(stats.commands_executed(), 0);
    assert_eq!(stats.versions_reclaimed(), 0);

    for _ in 0..100 {
        stats.record_mark();
    }

    assert_eq!(stats.marks_created(), 100);

    for _ in 0..95 {
        stats.record_execution();
    }

    assert_eq!(stats.commands_executed(), 95);

    for _ in 0..95 {
        stats.record_reclamation();
    }

    assert_eq!(stats.versions_reclaimed(), 95);
}

#[test]
fn test_large_scale_reclamation_scenario() {
    let status_table = TransactionStatusTable::new();
    let mut tx_ids = Vec::new();

    for id in 1..=100 {
        let tx_id = tx_id(id);
        if id % 2 == 0 {
            record_committed(&status_table, tx_id);
        }
        tx_ids.push(tx_id);
    }

    let mut eligible_marks = 0;
    for version_id in 1..=1000 {
        let tx_id = tx_ids[(version_id - 1) as usize % tx_ids.len()];

        let mark_opt = ReclamationMark::from_version_if_eligible(
            default_candidate(version_id, tx_id),
            &status_table,
            DEFAULT_MIN_VISIBLE_TS,
            0,
            DEFAULT_CURRENT_GC_EPOCH,
        );

        if let Ok(Some(_mark)) = mark_opt {
            eligible_marks += 1;
        }
    }

    assert_eq!(eligible_marks, 500);
}
