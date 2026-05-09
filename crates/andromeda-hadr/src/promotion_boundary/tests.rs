use super::*;
use crate::Lsn;

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
fn test_requirements_all_valid() {
    let req = make_requirements(1000, 950, true, true);
    assert!(req.validate().is_ok());
}

#[test]
fn test_requirements_lsn_behind() {
    let req = make_requirements(900, 950, true, true);
    assert!(req.validate().is_err());
    assert_eq!(req.lsn_gap(), 50);
}

#[test]
fn test_requirements_not_member() {
    let req = make_requirements(1000, 950, false, true);
    assert!(req.validate().is_err());
}

#[test]
fn test_requirements_no_connection() {
    let req = make_requirements(1000, 950, true, false);
    assert!(req.validate().is_err());
}

#[test]
fn test_durable_lsn_caught_up() {
    let req1 = make_requirements(1000, 950, true, true);
    assert!(req1.is_durable_lsn_caught_up());

    let req2 = make_requirements(1000, 1000, true, true);
    assert!(req2.is_durable_lsn_caught_up());

    let req3 = make_requirements(900, 950, true, true);
    assert!(!req3.is_durable_lsn_caught_up());
}

#[test]
fn test_lsn_gap_calculation() {
    let req1 = make_requirements(1000, 950, true, true);
    assert_eq!(req1.lsn_gap(), 0);

    let req2 = make_requirements(900, 1000, true, true);
    assert_eq!(req2.lsn_gap(), 100);

    let req3 = make_requirements(1000, 1000, true, true);
    assert_eq!(req3.lsn_gap(), 0);
}

#[test]
fn test_promotion_eligibility_functions() {
    let req_good = make_requirements(1000, 950, true, true);
    let eligibility_good = is_promotion_eligible(1, &req_good, 5, 0);
    assert!(eligibility_good.is_eligible());
    assert_eq!(eligibility_good.replica_id(), 1);

    let req_bad = make_requirements(900, 950, true, true);
    let eligibility_bad = is_promotion_eligible(2, &req_bad, 5, 1);
    assert!(!eligibility_bad.is_eligible());
    assert_eq!(eligibility_bad.replica_id(), 2);
}

#[test]
fn test_promotion_candidate_ranking() {
    let req1 = make_requirements(1000, 950, true, true);
    let req2 = make_requirements(950, 950, true, true);

    let c1 = PromotionCandidate::new(1, req1, 5, 0);
    let c2 = PromotionCandidate::new(2, req2, 5, 1);

    assert!(c1.is_higher_ranked_than(&c2));
    assert!(!c2.is_higher_ranked_than(&c1));
}

#[test]
fn test_select_best_eligible_candidate() -> andromeda_error::AndromedaResult<()> {
    let req_good = make_requirements(1000, 950, true, true);
    let req_bad = make_requirements(900, 950, true, true);

    let candidates = vec![
        PromotionCandidate::new(2, req_bad, 5, 0),
        PromotionCandidate::new(1, req_good, 5, 1),
    ];

    let selected = select_best_eligible_candidate(&candidates)?;
    assert_eq!(selected.replica_id, 1);
    Ok(())
}

#[test]
fn test_no_eligible_candidates() {
    let req_bad1 = make_requirements(900, 950, true, true);
    let req_bad2 = make_requirements(800, 950, true, true);

    let candidates = vec![
        PromotionCandidate::new(1, req_bad1, 5, 0),
        PromotionCandidate::new(2, req_bad2, 5, 1),
    ];

    let selected = select_best_eligible_candidate(&candidates);
    assert!(selected.is_err());
}

#[test]
fn test_failover_trigger_descriptions() {
    assert_eq!(
        FailoverTrigger::PrimaryUnreachable.as_str(),
        "primary is unreachable"
    );
    assert_eq!(
        FailoverTrigger::PrimaryHealthCheckFailed.as_str(),
        "primary failed health check"
    );
    assert_eq!(
        FailoverTrigger::ManualFailoverRequested.as_str(),
        "manual failover requested by operator"
    );
}
