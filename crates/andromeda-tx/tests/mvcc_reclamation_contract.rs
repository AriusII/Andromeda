//! MVCC Reclamation Contract Tests
//!
//! These tests verify the correctness of the MVCC reclamation mark system,
//! ensuring that only safe versions are marked for cleanup and that no
//! visible version can ever be reclaimed.

#[cfg(test)]
mod tests {
    use andromeda_core::TransactionId;
    use andromeda_tx::*;

    fn candidate(
        version_id: u64,
        creator_tx_id: TransactionId,
        end_ts: u64,
        marked_at: u64,
        gc_epoch: u64,
    ) -> ReclamationMarkCandidate {
        ReclamationMarkCandidate::new(version_id, creator_tx_id, end_ts, marked_at, gc_epoch)
    }

    // Mark creation for eligible versions.

    #[test]
    fn test_mark_created_for_eligible_version() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        // Set creator as committed
        status_table.set_committed(tx_id).unwrap();

        let mark_opt = ReclamationMark::from_version_if_eligible(
            candidate(1, tx_id, 50, 20, 5),
            &status_table,
            100, // min_visible_ts (end_ts < min_visible_ts)
            0,   // grace_period_epochs (no grace period)
            10,  // current_gc_epoch
        );

        assert!(mark_opt.is_ok(), "Should create mark without error");
        assert!(
            mark_opt.unwrap().is_some(),
            "Should return Some when fully eligible"
        );
    }

    // Rejection for an uncommitted creator.

    #[test]
    fn test_mark_rejected_uncommitted_creator() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        // Don't mark as committed - remains InFlight

        let mark_opt = ReclamationMark::from_version_if_eligible(
            candidate(1, tx_id, 50, 20, 5),
            &status_table,
            100,
            0,
            10,
        );

        assert!(mark_opt.is_ok());
        assert!(
            mark_opt.unwrap().is_none(),
            "Should not create mark for uncommitted creator"
        );
    }

    // Active snapshots keep still-visible versions out of reclamation.

    #[test]
    fn test_mark_rejected_version_still_visible() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        status_table.set_committed(tx_id).unwrap();

        // Version with end_ts = 150
        // min_visible_ts = 100
        // This means an active snapshot at ts=100 can still see this version
        // (versions are visible if begin_ts <= snapshot_ts AND end_ts > snapshot_ts)

        let mark_opt = ReclamationMark::from_version_if_eligible(
            candidate(1, tx_id, 150, 20, 5),
            &status_table,
            100,
            0,
            10,
        );

        assert!(mark_opt.is_ok());
        assert!(
            mark_opt.unwrap().is_none(),
            "Should not mark version still visible to active snapshots"
        );
    }

    // Grace-period enforcement.

    #[test]
    fn test_mark_rejected_grace_period_not_expired() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        status_table.set_committed(tx_id).unwrap();

        // Grace period is 5 epochs
        // Version was marked at gc_epoch=10
        // Current gc_epoch=10
        // Not eligible: 10 < 10 + 5

        let mark_opt = ReclamationMark::from_version_if_eligible(
            candidate(1, tx_id, 50, 20, 10),
            &status_table,
            100,
            5,
            10,
        );

        assert!(mark_opt.is_ok());
        assert!(
            mark_opt.unwrap().is_none(),
            "Should reject mark when grace period not expired"
        );
    }

    // Reclamation command creation.

    #[test]
    fn test_mark_emits_reclamation_command() {
        let mark = ReclamationMark::new(1, TransactionId::new(1), 100, 50, 5).unwrap();

        let cmd = mark.mark_for_reclamation();

        assert_eq!(cmd.version_id, 1);
        assert_eq!(cmd.creator_tx_id, TransactionId::new(1));
        assert_eq!(cmd.end_ts, 100);
    }

    // Reclamation mark set processing.

    #[test]
    fn test_batch_reclamation_100_marks() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        status_table.set_committed(tx_id).unwrap();

        let mut marks = Vec::new();

        // Create 100 marks with different version_ids
        for i in 1..=100 {
            let mark_opt = ReclamationMark::from_version_if_eligible(
                candidate(i, tx_id, i * 10, 20, 5),
                &status_table,
                2000, // min_visible_ts (all end_ts < 2000)
                0,    // grace_period_epochs
                10,   // current_gc_epoch
            );

            assert!(mark_opt.is_ok());
            if let Ok(Some(mark)) = mark_opt {
                marks.push(mark);
            }
        }

        assert_eq!(marks.len(), 100, "Should create all 100 marks");

        // Verify all marks are distinct
        let mut version_ids: Vec<u64> = marks.iter().map(|m| m.version_id).collect();
        version_ids.sort();
        version_ids.dedup();
        assert_eq!(
            version_ids.len(),
            100,
            "All marks should have distinct version_ids"
        );
    }

    // Visible-version safety.

    #[test]
    fn test_no_visible_version_can_be_reclaimed() {
        let status_table = TransactionStatusTable::new();
        let creator_tx_id = TransactionId::new(1);

        status_table.set_committed(creator_tx_id).unwrap();

        // Scenario:
        // - Version with end_ts = 100
        // - Active snapshots range from ts=100 to ts=200
        // - min_visible_ts = 100
        //
        // This version CANNOT be marked because end_ts (100) is NOT < min_visible_ts (100)
        // Therefore, active snapshots at ts=100 can potentially see it

        let mark_opt = ReclamationMark::from_version_if_eligible(
            candidate(1, creator_tx_id, 100, 20, 5),
            &status_table,
            100, // min_visible_ts
            0,
            10,
        );

        assert!(mark_opt.is_ok());
        assert!(
            mark_opt.unwrap().is_none(),
            "Version at boundary should not be reclaimed"
        );

        // But if min_visible_ts advances to 101, now it's safe
        let mark_opt2 = ReclamationMark::from_version_if_eligible(
            candidate(1, creator_tx_id, 100, 20, 5),
            &status_table,
            101, // min_visible_ts advanced
            0,
            10,
        );

        assert!(mark_opt2.is_ok());
        assert!(
            mark_opt2.unwrap().is_some(),
            "Version becomes reclaimable when end_ts < min_visible_ts"
        );
    }

    // Eligibility criteria structure.

    #[test]
    fn test_eligibility_criteria_combinations() {
        let eligibility_scenarios = vec![
            (true, true, true, true),     // All met
            (false, true, true, false),   // Creator not committed
            (true, false, true, false),   // Version still visible
            (true, true, false, false),   // Grace period not met
            (false, false, false, false), // None met
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

    // Mark validation.

    #[test]
    fn test_mark_validation_rejects_invalid_inputs() {
        // Zero version_id
        let result1 = ReclamationMark::new(0, TransactionId::new(1), 100, 50, 5);
        assert!(result1.is_err());

        // Zero creator_tx_id
        let result2 = ReclamationMark::new(1, TransactionId::new(0), 100, 50, 5);
        assert!(result2.is_err());

        // Live version (end_ts == u64::MAX)
        let result3 = ReclamationMark::new(1, TransactionId::new(1), u64::MAX, 50, 5);
        assert!(result3.is_err());

        // Valid
        let result4 = ReclamationMark::new(1, TransactionId::new(1), 100, 50, 5);
        assert!(result4.is_ok());
    }

    // Runtime eligibility checks.

    #[test]
    fn test_mark_runtime_eligibility_check() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        status_table.set_committed(tx_id).unwrap();

        let mark = ReclamationMark::new(1, tx_id, 50, 20, 5).unwrap();

        // Initially eligible
        assert!(mark.is_eligible(&status_table, 100, 0, 10));

        // Still eligible after various checks
        let eligibility1 = mark.check_eligibility(&status_table, 100, 0, 10);
        assert!(eligibility1.is_fully_eligible());

        // Not eligible if min_visible_ts drops below end_ts (shouldn't happen, but check)
        let eligibility2 = mark.check_eligibility(&status_table, 40, 0, 10);
        assert!(!eligibility2.is_end_ts_invisible);
        assert!(!eligibility2.is_fully_eligible());
    }

    // Multiple-creator scenarios.

    #[test]
    fn test_reclamation_multiple_creators() {
        let status_table = TransactionStatusTable::new();

        // Create 3 transactions
        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);
        let tx3 = TransactionId::new(3);

        // Mark tx1 and tx3 as committed, tx2 remains InFlight
        status_table.set_committed(tx1).unwrap();
        status_table.set_committed(tx3).unwrap();

        // Try to create marks for all three
        let mark1 = ReclamationMark::from_version_if_eligible(
            candidate(1, tx1, 50, 20, 5),
            &status_table,
            100,
            0,
            10,
        );
        let mark2 = ReclamationMark::from_version_if_eligible(
            candidate(2, tx2, 50, 20, 5),
            &status_table,
            100,
            0,
            10,
        );
        let mark3 = ReclamationMark::from_version_if_eligible(
            candidate(3, tx3, 50, 20, 5),
            &status_table,
            100,
            0,
            10,
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

    // Grace-period scenarios.

    #[test]
    fn test_grace_period_multiple_epochs() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        status_table.set_committed(tx_id).unwrap();

        let grace_period_epochs = 10;

        // Mark created at epoch 5
        let mark = ReclamationMark::new(1, tx_id, 50, 20, 5).unwrap();

        // At epoch 10: 10 < 5 + 10 = not eligible
        let elig1 = mark.check_eligibility(&status_table, 100, grace_period_epochs, 10);
        assert!(!elig1.gc_epoch_qualified);

        // At epoch 14: 14 < 5 + 10 = not eligible
        let elig2 = mark.check_eligibility(&status_table, 100, grace_period_epochs, 14);
        assert!(!elig2.gc_epoch_qualified);

        // At epoch 15: 15 >= 5 + 10 = eligible
        let elig3 = mark.check_eligibility(&status_table, 100, grace_period_epochs, 15);
        assert!(elig3.gc_epoch_qualified);

        // At epoch 100: 100 >= 5 + 10 = eligible
        let elig4 = mark.check_eligibility(&status_table, 100, grace_period_epochs, 100);
        assert!(elig4.gc_epoch_qualified);
    }

    // Reclamation stats tracking.

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

    // Exact-boundary behavior.

    #[test]
    fn test_version_at_exact_min_visible_ts_boundary() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        status_table.set_committed(tx_id).unwrap();

        // Version with end_ts exactly at min_visible_ts boundary
        // If end_ts = min_visible_ts, a snapshot at that exact timestamp could see it
        // So it must NOT be marked for reclamation

        let mark_opt = ReclamationMark::from_version_if_eligible(
            candidate(1, tx_id, 100, 20, 5),
            &status_table,
            100, // end_ts == min_visible_ts
            0,
            10,
        );

        assert!(mark_opt.is_ok());
        assert!(
            mark_opt.unwrap().is_none(),
            "Version at boundary should not be marked"
        );
    }

    // Large-scale scenarios.

    #[test]
    fn test_large_scale_reclamation_scenario() {
        let status_table = TransactionStatusTable::new();

        // Create 100 transactions
        let mut tx_ids = Vec::new();
        for i in 1..=100 {
            let tx_id = TransactionId::new(i as u64);
            // Only mark even-numbered transactions as committed
            if i % 2 == 0 {
                status_table.set_committed(tx_id).unwrap();
            }
            tx_ids.push(tx_id);
        }

        // Try to create 1000 marks from these transactions
        let mut eligible_marks = 0;
        for version_id in 1..=1000 {
            let tx_idx = (version_id - 1) % tx_ids.len();
            let tx_id = tx_ids[tx_idx];

            let mark_opt = ReclamationMark::from_version_if_eligible(
                candidate(version_id as u64, tx_id, 50, 20, 5),
                &status_table,
                100,
                0,
                10,
            );

            if let Ok(Some(_mark)) = mark_opt {
                eligible_marks += 1;
            }
        }

        // Should be ~500 eligible marks (from even-numbered committed transactions)
        assert!(
            eligible_marks > 400 && eligible_marks < 600,
            "Approximately 500 marks should be eligible, got {}",
            eligible_marks
        );
    }

    // Mark command generation.

    #[test]
    fn test_reclamation_command_generation() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        status_table.set_committed(tx_id).unwrap();

        let mark_opt = ReclamationMark::from_version_if_eligible(
            candidate(42, tx_id, 500, 100, 10),
            &status_table,
            1000,
            0,
            20,
        );

        assert!(mark_opt.is_ok());
        let mark = mark_opt.unwrap().unwrap();

        let cmd = mark.mark_for_reclamation();

        assert_eq!(cmd.version_id, 42);
        assert_eq!(cmd.creator_tx_id, tx_id);
        assert_eq!(cmd.end_ts, 500);
    }
}
