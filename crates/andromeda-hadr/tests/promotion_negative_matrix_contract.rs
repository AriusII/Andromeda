use andromeda_hadr::{
    HadrEpoch, HadrFencingContext, HadrFencingRejection, HadrFencingToken, HadrNodeId,
    HadrNodeRole, HadrNodeState, HadrPromotionOutcome, HadrPromotionRejection,
    HadrPromotionRequest, HadrPromotionVote, HadrQuorumMembership, enforce_fencing_token,
    evaluate_promotion,
};
use andromeda_wal::Lsn;

fn membership_three_nodes() -> HadrQuorumMembership {
    HadrQuorumMembership::new(vec![
        HadrNodeId::new(1),
        HadrNodeId::new(2),
        HadrNodeId::new(3),
    ])
    .expect("membership")
}

fn candidate(observed_epoch: u64, safe_lsn: u64) -> HadrNodeState {
    HadrNodeState::new(
        HadrNodeId::new(2),
        HadrNodeRole::Replica,
        HadrEpoch::new(observed_epoch),
        Lsn::new(safe_lsn),
    )
}

fn grant(voter: u64, observed_epoch: u64, safe_lsn: u64) -> HadrPromotionVote {
    HadrPromotionVote::grant(
        HadrNodeId::new(voter),
        HadrEpoch::new(observed_epoch),
        Lsn::new(safe_lsn),
    )
}

fn assert_rejected(
    request: HadrPromotionRequest,
    fencing: HadrFencingContext,
    expected: HadrPromotionRejection,
) {
    let membership = membership_three_nodes();
    let audit = evaluate_promotion(&request, &membership, &fencing);
    assert_eq!(audit.outcome, HadrPromotionOutcome::Rejected(expected));
}

#[test]
fn rejects_stale_candidate_epoch_boundary() {
    assert_rejected(
        HadrPromotionRequest::new(
            candidate(5, 100),
            HadrEpoch::new(5),
            vec![grant(1, 4, 100), grant(3, 4, 100)],
        ),
        HadrFencingContext {
            active_token: None,
            highest_observed_epoch: HadrEpoch::new(5),
        },
        HadrPromotionRejection::EpochNotMonotonic,
    );
}

#[test]
fn rejects_stale_voter_epoch_boundary() {
    assert_rejected(
        HadrPromotionRequest::new(
            candidate(5, 100),
            HadrEpoch::new(6),
            vec![grant(1, 6, 100), grant(3, 5, 100)],
        ),
        HadrFencingContext::empty(),
        HadrPromotionRejection::EpochNotMonotonic,
    );
}

#[test]
fn rejects_duplicate_voters() {
    assert_rejected(
        HadrPromotionRequest::new(
            candidate(5, 100),
            HadrEpoch::new(6),
            vec![grant(1, 5, 100), grant(1, 5, 100)],
        ),
        HadrFencingContext::empty(),
        HadrPromotionRejection::InvalidVoteRoster,
    );
}

#[test]
fn rejects_non_member_voters() {
    assert_rejected(
        HadrPromotionRequest::new(
            candidate(5, 100),
            HadrEpoch::new(6),
            vec![grant(1, 5, 100), grant(42, 5, 100)],
        ),
        HadrFencingContext::empty(),
        HadrPromotionRejection::InvalidVoteRoster,
    );
}

#[test]
fn rejects_without_quorum() {
    let denied = HadrPromotionVote::deny(HadrNodeId::new(3), HadrEpoch::new(5), Lsn::new(100));
    assert_rejected(
        HadrPromotionRequest::new(
            candidate(5, 100),
            HadrEpoch::new(6),
            vec![grant(1, 5, 100), denied],
        ),
        HadrFencingContext::empty(),
        HadrPromotionRejection::InsufficientQuorum,
    );
}

#[test]
fn rejects_stale_candidate_lsn() {
    assert_rejected(
        HadrPromotionRequest::new(
            candidate(5, 90),
            HadrEpoch::new(6),
            vec![grant(1, 5, 100), grant(3, 5, 95)],
        ),
        HadrFencingContext::empty(),
        HadrPromotionRejection::StaleCandidateLsn,
    );
}

#[test]
fn rejects_active_token_conflict_double_promotion() {
    assert_rejected(
        HadrPromotionRequest::new(
            candidate(8, 200),
            HadrEpoch::new(9),
            vec![grant(1, 8, 200), grant(3, 8, 200)],
        ),
        HadrFencingContext::with_active(
            HadrFencingToken::new(HadrNodeId::new(1), HadrEpoch::new(9)),
            HadrEpoch::new(8),
        ),
        HadrPromotionRejection::SplitBrainActiveToken,
    );
}

#[test]
fn rejects_operations_without_fencing_proof() {
    let err = enforce_fencing_token(
        HadrFencingToken::new(HadrNodeId::new(2), HadrEpoch::new(2)),
        &HadrFencingContext::empty(),
    )
    .expect_err("operation must fail without active fencing token");

    assert_eq!(err.message(), HadrFencingRejection::NoActiveToken.as_str());
}
