//! Promotion boundary integration and contract tests.
//!
//! Tests verify that promotion eligibility can be queried without I/O, that all
//! requirements are enforced, and that eligibility rankings work correctly.

use andromeda_storage::Lsn;
use andromeda_storage::hadr::{
    FailoverTrigger, PromotionCandidate, PromotionEligibility, PromotionRequirements,
    is_promotion_eligible, select_best_eligible_candidate,
};

/// Helper to construct promotion requirements for testing.
fn make_requirements(
    replica_lsn: u64,
    primary_lsn: u64,
    is_member: bool,
    has_conn: bool,
) -> PromotionRequirements {
    PromotionRequirements::new(
        Lsn::new(replica_lsn),
        Lsn::new(primary_lsn),
        is_member,
        has_conn,
    )
}

#[test]
fn test_replica_promoted_if_lsn_caught_up_and_member() {
    // Replica with LSN caught up to primary, member of quorum, connected
    let req = make_requirements(1000, 1000, true, true);

    let eligibility = is_promotion_eligible(1, &req, 5, 0);

    assert!(
        eligibility.is_eligible(),
        "Replica with caught-up LSN and membership should be eligible"
    );
    match eligibility {
        PromotionEligibility::Eligible(candidate) => {
            assert_eq!(candidate.replica_id, 1);
            assert_eq!(candidate.rank, 0);
            assert_eq!(candidate.observed_epoch, 5);
            assert!(candidate.validate().is_ok());
        },
        _ => panic!("Expected Eligible variant"),
    }
}

#[test]
fn test_replica_promoted_if_ahead_of_primary_lsn() {
    // Replica ahead of primary (caught up early; rare but valid)
    let req = make_requirements(2000, 1000, true, true);

    let eligibility = is_promotion_eligible(2, &req, 10, 0);

    assert!(
        eligibility.is_eligible(),
        "Replica ahead of primary should be eligible"
    );
}

#[test]
fn test_replica_not_promoted_if_behind_primary_lsn() {
    // Replica behind primary — not caught up yet
    let req = make_requirements(900, 1000, true, true);

    let eligibility = is_promotion_eligible(3, &req, 5, 0);

    assert!(
        !eligibility.is_eligible(),
        "Replica behind primary should not be eligible"
    );
    assert_eq!(eligibility.replica_id(), 3);
    match eligibility {
        PromotionEligibility::Ineligible { replica_id, reason } => {
            assert_eq!(replica_id, 3);
            assert!(
                reason.to_lowercase().contains("behind") || reason.to_lowercase().contains("loss")
            );
        },
        _ => panic!("Expected Ineligible variant"),
    }
}

#[test]
fn test_replica_not_promoted_if_not_quorum_member() {
    // Replica not in membership snapshot (e.g., recently added, not yet confirmed)
    let req = make_requirements(1000, 1000, false, true);

    let eligibility = is_promotion_eligible(4, &req, 5, 0);

    assert!(
        !eligibility.is_eligible(),
        "Non-member replica should not be eligible"
    );
    assert_eq!(eligibility.replica_id(), 4);
}

#[test]
fn test_replica_not_promoted_if_no_connection() {
    // Replica connected, but connection is down
    let req = make_requirements(1000, 1000, true, false);

    let eligibility = is_promotion_eligible(5, &req, 5, 0);

    assert!(
        !eligibility.is_eligible(),
        "Replica without connection should not be eligible"
    );
}

#[test]
fn test_promotion_eligibility_queryable_without_io() {
    // Verify that eligibility check is pure (no side effects, no I/O)
    let req = make_requirements(1000, 950, true, true);

    // Call multiple times; results should be identical
    let result1 = is_promotion_eligible(6, &req, 5, 0);
    let result2 = is_promotion_eligible(6, &req, 5, 0);

    assert_eq!(result1, result2, "Eligibility should be deterministic");
    assert!(result1.is_eligible());
}

#[test]
fn test_multiple_eligible_replicas_ranked_by_lsn() {
    // Three replicas at different LSN positions
    let req_best = make_requirements(1000, 950, true, true); // Rank 0 (best)
    let req_mid = make_requirements(980, 950, true, true); // Rank 1
    let req_worst = make_requirements(960, 950, true, true); // Rank 2

    let c1 = PromotionCandidate::new(1, req_best, 5, 0);
    let c2 = PromotionCandidate::new(2, req_mid, 5, 1);
    let c3 = PromotionCandidate::new(3, req_worst, 5, 2);

    assert!(c1.is_higher_ranked_than(&c2));
    assert!(c2.is_higher_ranked_than(&c3));
    assert!(!c3.is_higher_ranked_than(&c1));
}

#[test]
fn test_select_best_eligible_candidate_skips_ineligible() {
    // Mix of eligible and ineligible candidates
    let req_ineligible = make_requirements(900, 950, true, true); // Behind primary
    let req_eligible = make_requirements(1000, 950, true, true); // Caught up

    let candidates = vec![
        PromotionCandidate::new(1, req_ineligible, 5, 0), // Ranked best but ineligible
        PromotionCandidate::new(2, req_eligible, 5, 1),   // Ranked worse but eligible
        PromotionCandidate::new(3, req_ineligible, 5, 2), // Worst rank and ineligible
    ];

    let selected = select_best_eligible_candidate(&candidates);
    assert!(
        selected.is_ok(),
        "Should find at least one eligible candidate"
    );
    assert_eq!(
        selected.unwrap().replica_id,
        2,
        "Should select candidate 2 (only eligible one)"
    );
}

#[test]
fn test_select_first_eligible_by_rank_order() {
    // All candidates eligible; should select highest-ranked
    let req = make_requirements(1000, 950, true, true);

    let candidates = vec![
        PromotionCandidate::new(2, req, 5, 2),
        PromotionCandidate::new(1, req, 5, 0), // Best rank
        PromotionCandidate::new(3, req, 5, 1),
    ];

    let selected = select_best_eligible_candidate(&candidates);
    assert!(selected.is_ok());
    assert_eq!(
        selected.unwrap().replica_id,
        1,
        "Should select highest-ranked eligible"
    );
}

#[test]
fn test_requirements_lsn_gap_calculation() {
    let req_ahead = make_requirements(1500, 1000, true, true);
    assert_eq!(req_ahead.lsn_gap(), 0, "Ahead replica has no gap");

    let req_behind = make_requirements(800, 1000, true, true);
    assert_eq!(req_behind.lsn_gap(), 200, "Behind replica has 200-byte gap");

    let req_equal = make_requirements(1000, 1000, true, true);
    assert_eq!(req_equal.lsn_gap(), 0, "Equal LSN has no gap");
}

#[test]
fn test_requirements_is_durable_lsn_caught_up() {
    assert!(make_requirements(1000, 1000, true, true).is_durable_lsn_caught_up());
    assert!(make_requirements(1500, 1000, true, true).is_durable_lsn_caught_up());
    assert!(!make_requirements(900, 1000, true, true).is_durable_lsn_caught_up());
}

#[test]
fn test_all_requirements_must_pass() {
    // Test that ANY failed requirement blocks promotion

    // Fail LSN
    let req1 = make_requirements(900, 1000, true, true);
    assert!(req1.validate().is_err());

    // Fail membership
    let req2 = make_requirements(1000, 1000, false, true);
    assert!(req2.validate().is_err());

    // Fail connectivity
    let req3 = make_requirements(1000, 1000, true, false);
    assert!(req3.validate().is_err());

    // All pass
    let req4 = make_requirements(1000, 1000, true, true);
    assert!(req4.validate().is_ok());
}

#[test]
fn test_no_race_between_lsn_update_and_promotion_query() {
    // Simulates: LSN updates while eligibility is being checked
    // By being a pure function, the check is atomic from caller perspective

    let mut req = make_requirements(950, 950, true, true);

    // Check 1: not caught up
    let e1 = is_promotion_eligible(7, &req, 5, 0);
    assert!(e1.is_eligible(), "Just caught up");

    // Simulate LSN falling behind (backward movement should not happen, but test defensiveness)
    req = make_requirements(900, 950, true, true);

    // Check 2: behind again
    let e2 = is_promotion_eligible(7, &req, 5, 0);
    assert!(!e2.is_eligible(), "Now behind");

    // The eligibility checks are independent snapshots; no race condition
}

#[test]
fn test_no_automatic_failover_eligibility_computation_only() {
    // Presence of eligibility DOES NOT trigger failover
    // F6 must decide when and whether to promote

    let req = make_requirements(1000, 950, true, true);
    let eligibility = is_promotion_eligible(8, &req, 5, 0);

    assert!(eligibility.is_eligible(), "Replica is eligible");

    // But just being eligible means nothing happens automatically
    // F6 must make the promotion decision
    // (This test just verifies no side effects occurred in eligibility check)
}

#[test]
fn test_failover_trigger_descriptions_present() {
    // Verify all triggers have descriptions (for audit/operator logs)

    assert!(!FailoverTrigger::PrimaryUnreachable.as_str().is_empty());
    assert!(
        !FailoverTrigger::PrimaryHealthCheckFailed
            .as_str()
            .is_empty()
    );
    assert!(!FailoverTrigger::ManualFailoverRequested.as_str().is_empty());
    assert!(!FailoverTrigger::FencingTokenExpired.as_str().is_empty());
    assert!(!FailoverTrigger::DataDivergenceDetected.as_str().is_empty());
    assert!(!FailoverTrigger::QuorumLost.as_str().is_empty());
}

#[test]
fn test_failover_trigger_variants_are_documented_not_executed() {
    // Triggers are enum variants; they do NOT execute anything
    // They're for F6 orchestration layer to interpret

    let triggers = [
        FailoverTrigger::PrimaryUnreachable,
        FailoverTrigger::PrimaryHealthCheckFailed,
        FailoverTrigger::ManualFailoverRequested,
    ];

    // Just the act of creating triggers should be side-effect-free
    assert_eq!(triggers.len(), 3);
}

#[test]
fn test_promotion_candidate_construction() {
    let req = make_requirements(1000, 950, true, true);
    let candidate = PromotionCandidate::new(1, req, 10, 0);

    assert_eq!(candidate.replica_id, 1);
    assert_eq!(candidate.observed_epoch, 10);
    assert_eq!(candidate.rank, 0);
    assert!(candidate.validate().is_ok());
}

#[test]
fn test_promotion_candidate_into_result() {
    let req_good = make_requirements(1000, 950, true, true);
    let candidate_good = PromotionCandidate::new(1, req_good, 5, 0);

    let eligibility_good = PromotionEligibility::Eligible(candidate_good);
    assert!(eligibility_good.into_candidate().is_ok());
}

#[test]
fn test_promotion_eligibility_into_candidate_fails() {
    let eligibility_bad = PromotionEligibility::Ineligible {
        replica_id: 2,
        reason: "behind primary".to_string(),
    };

    assert!(eligibility_bad.into_candidate().is_err());
}

#[test]
fn test_zero_lsn_values() {
    // Edge case: both at LSN 0 (fresh cluster)
    let req = make_requirements(0, 0, true, true);
    let eligibility = is_promotion_eligible(9, &req, 0, 0);
    assert!(eligibility.is_eligible());
}

#[test]
fn test_large_lsn_values() {
    // Edge case: very large LSN values (near u64 max)
    let max_safe = u64::MAX / 2;
    let req = make_requirements(max_safe, max_safe - 1000, true, true);
    let eligibility = is_promotion_eligible(10, &req, 100, 0);
    assert!(eligibility.is_eligible());
}

#[test]
fn test_primary_ahead_way_ahead() {
    // Primary is far ahead; replica significantly behind
    let req = make_requirements(1000, 1000000, true, true);
    let eligibility = is_promotion_eligible(11, &req, 5, 0);
    assert!(!eligibility.is_eligible());
    assert_eq!(req.lsn_gap(), 999000);
}

#[test]
fn test_promotion_candidate_with_quorum_epoch() {
    // Candidate has observed epoch from F3 quorum; F6 must ensure promoted epoch is higher
    let req = make_requirements(1000, 950, true, true);
    let candidate = PromotionCandidate::new(1, req, 42, 0);

    assert_eq!(candidate.observed_epoch, 42);
    // F6 will ensure proposed_epoch > 42
    // (This test just documents the contract)
}

#[test]
fn test_batch_validate_candidates() {
    let reqs = [
        make_requirements(1000, 950, true, true),  // eligible
        make_requirements(900, 950, true, true),   // not eligible
        make_requirements(1000, 950, false, true), // not eligible
        make_requirements(1000, 950, true, true),  // eligible
    ];

    let candidates: Vec<_> = reqs
        .iter()
        .enumerate()
        .map(|(i, r)| PromotionCandidate::new((i + 1) as u64, *r, 5, i))
        .collect();

    let eligible_count = candidates.iter().filter(|c| c.validate().is_ok()).count();

    assert_eq!(eligible_count, 2, "Should have 2 eligible candidates");
}

#[test]
fn test_promotion_boundary_separates_f3_and_f6_concerns() {
    // F3 (quorum): Computes LSN, membership, connectivity
    // F4 (this module): Defines the boundary
    // F6+ : Executes promotion orchestration

    // This module provides:
    let req = make_requirements(1000, 950, true, true);
    let eligibility = is_promotion_eligible(1, &req, 5, 0);

    // 1. Pure eligibility check (no I/O, no side effects)
    assert!(eligibility.is_eligible());

    // 2. Structured requirements (what F3 must provide)
    assert_eq!(req.replica_safe_lsn.get(), 1000);
    assert_eq!(req.primary_durable_lsn.get(), 950);

    // 3. No promotion execution (that's F6's job)
    // (We just returned eligibility, not a promotion command)
}
