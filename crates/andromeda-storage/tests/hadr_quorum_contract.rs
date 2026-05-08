use std::error::Error;

use andromeda_storage::{
    Lsn,
    hadr::{
        HadrEpoch, HadrFencingContext, HadrFencingToken, HadrNodeId, HadrNodeRole, HadrNodeState,
        HadrPromotionOutcome, HadrPromotionRejection, HadrPromotionRequest, HadrPromotionVote,
        HadrQuorumMembership, enforce_fencing_token, evaluate_promotion,
        quorum_runtime::{
            FencingDecision, FencingEvent, FencingPolicy, QuorumConsensus, QuorumMembership,
            ReplicaMember, ReplicationMode, decide_fencing,
        },
        shipping_runtime::{ShippingBackpressureRequest, ShippingCondition},
    },
    write_ahead_log::{
        WalNodeIdentity, WalNodeRole, WalRecord, WalRecordKind, WalReplicaExpectation,
        WalReplicaSafeLsnTracker, WalShipmentBatch, WalShippingAck,
    },
};

type TestResult = Result<(), Box<dyn Error>>;

fn wal_record(lsn: u64, previous_lsn: Option<u64>) -> Result<WalRecord, Box<dyn Error>> {
    Ok(WalRecord::from_parts(
        WalRecordKind::PageAllocate,
        Lsn::new(lsn),
        previous_lsn.map(Lsn::new),
        None,
        Vec::<u8>::new(),
    )?)
}

#[test]
fn promotion_approval_advances_epoch_and_fences_prior_primary() -> TestResult {
    let old_primary = HadrNodeId::new(1);
    let candidate_id = HadrNodeId::new(2);
    let witness_id = HadrNodeId::new(3);
    let proposed_epoch = HadrEpoch::new(8);
    let previous_epoch = HadrEpoch::new(7);

    let membership = HadrQuorumMembership::new(vec![old_primary, candidate_id, witness_id])?;
    let fencing = HadrFencingContext::with_active(
        HadrFencingToken::new(old_primary, previous_epoch),
        previous_epoch,
    );
    let candidate = HadrNodeState::new(
        candidate_id,
        HadrNodeRole::Replica,
        previous_epoch,
        Lsn::new(500),
    );
    let request = HadrPromotionRequest::new(
        candidate,
        proposed_epoch,
        vec![
            HadrPromotionVote::grant(old_primary, previous_epoch, Lsn::new(500)),
            HadrPromotionVote::grant(witness_id, previous_epoch, Lsn::new(490)),
        ],
    );

    let audit = evaluate_promotion(&request, &membership, &fencing);

    assert_eq!(audit.quorum_size, 2);
    assert_eq!(audit.granted_votes, 2);
    assert_eq!(audit.highest_observed_epoch, previous_epoch);
    assert_eq!(
        audit.active_token,
        Some(HadrFencingToken::new(old_primary, previous_epoch))
    );
    assert_eq!(
        audit.outcome,
        HadrPromotionOutcome::Approved {
            token: HadrFencingToken::new(candidate_id, proposed_epoch),
            committed_safe_lsn: Lsn::new(500),
            granted_voters: vec![old_primary, witness_id],
        }
    );

    let new_context = HadrFencingContext::with_active(
        HadrFencingToken::new(candidate_id, proposed_epoch),
        proposed_epoch,
    );
    let stale_old_primary = enforce_fencing_token(
        HadrFencingToken::new(old_primary, previous_epoch),
        &new_context,
    );

    assert!(stale_old_primary.is_err());
    Ok(())
}

#[test]
fn promotion_rejects_active_token_at_proposed_epoch() -> TestResult {
    let active_primary = HadrNodeId::new(1);
    let candidate_id = HadrNodeId::new(2);
    let witness_id = HadrNodeId::new(3);
    let proposed_epoch = HadrEpoch::new(9);

    let membership = HadrQuorumMembership::new(vec![active_primary, candidate_id, witness_id])?;
    let fencing = HadrFencingContext::with_active(
        HadrFencingToken::new(active_primary, proposed_epoch),
        HadrEpoch::new(8),
    );
    let candidate = HadrNodeState::new(
        candidate_id,
        HadrNodeRole::Replica,
        HadrEpoch::new(8),
        Lsn::new(700),
    );
    let request = HadrPromotionRequest::new(
        candidate,
        proposed_epoch,
        vec![
            HadrPromotionVote::grant(active_primary, HadrEpoch::new(8), Lsn::new(700)),
            HadrPromotionVote::grant(witness_id, HadrEpoch::new(8), Lsn::new(700)),
        ],
    );

    let audit = evaluate_promotion(&request, &membership, &fencing);

    assert_eq!(
        audit.outcome,
        HadrPromotionOutcome::Rejected(HadrPromotionRejection::SplitBrainActiveToken)
    );
    Ok(())
}

#[test]
fn quorum_runtime_blocks_write_admission_after_quorum_loss() -> TestResult {
    let replicas = vec![
        ReplicaMember::new(2, Lsn::new(100), Lsn::new(100)),
        ReplicaMember::new(3, Lsn::new(100), Lsn::new(100)),
        ReplicaMember::new(4, Lsn::new(100), Lsn::new(100)),
    ];
    let mut membership = QuorumMembership::new(1, replicas)?;
    let consensus = QuorumConsensus::from_membership_majority(membership.size());

    assert_eq!(membership.quorum_size(), 2);
    assert!(membership.has_quorum());
    assert!(consensus.can_admit_write(2));

    membership.mark_dead(2)?;
    membership.mark_dead(3)?;

    assert!(!membership.has_quorum());
    assert!(!consensus.can_admit_write(membership.count_alive()));
    assert_eq!(
        decide_fencing(
            &membership,
            FencingEvent::ReplicaDisconnected,
            FencingPolicy::BlockOnQuorumLoss,
            ReplicationMode::QuorumEnforced,
        ),
        FencingDecision::Block
    );
    assert_eq!(
        decide_fencing(
            &membership,
            FencingEvent::ReplicaDisconnected,
            FencingPolicy::BlockOnQuorumLoss,
            ReplicationMode::Asynchronous,
        ),
        FencingDecision::Allow
    );

    Ok(())
}

#[test]
fn wal_shipping_ack_requires_durable_contiguous_validated_batch() -> TestResult {
    let records = vec![
        wal_record(1, None)?,
        wal_record(2, Some(1))?,
        wal_record(3, Some(2))?,
    ];

    assert!(
        !ShippingCondition {
            primary_durable_lsn: Lsn::new(2),
            segment_end_lsn: Lsn::new(3),
        }
        .is_shippable()
    );
    assert!(
        ShippingCondition {
            primary_durable_lsn: Lsn::new(3),
            segment_end_lsn: Lsn::new(3),
        }
        .is_shippable()
    );

    let batch = WalShipmentBatch::new(
        WalNodeIdentity::new(1, WalNodeRole::Primary),
        WalNodeIdentity::new(2, WalNodeRole::Replica),
        WalReplicaExpectation::genesis(Lsn::new(1)),
        &records,
    );
    let accepted = batch.validate()?;

    assert_eq!(accepted.range.first, Lsn::new(1));
    assert_eq!(accepted.range.last, Lsn::new(3));
    assert_eq!(accepted.next_expected_lsn, Lsn::new(4));

    let mut tracker = WalReplicaSafeLsnTracker::new([2, 3])?;
    tracker.record_ack(WalShippingAck::new(2, accepted.range.last))?;

    assert_eq!(tracker.replica_safe_lsn(2), Some(Lsn::new(3)));
    assert_eq!(tracker.replica_safe_lsn(3), Some(Lsn::ZERO));
    assert!(!tracker.all_required_replicas_have_shipped(accepted.range.last));

    tracker.record_ack(WalShippingAck::new(3, accepted.range.last))?;

    assert!(tracker.all_required_replicas_have_shipped(accepted.range.last));
    assert_eq!(tracker.retention_boundary_lsn(), Lsn::new(3));

    Ok(())
}

#[test]
fn wal_shipping_gap_rejects_batch_and_produces_backpressure_coordinates() -> TestResult {
    let records = vec![wal_record(5, Some(4))?, wal_record(7, Some(5))?];
    let batch = WalShipmentBatch::new(
        WalNodeIdentity::new(1, WalNodeRole::Primary),
        WalNodeIdentity::new(2, WalNodeRole::Replica),
        WalReplicaExpectation::after(Lsn::new(4), Lsn::new(5)),
        &records,
    );

    let rejection = batch
        .validate()
        .expect_err("numeric LSN gap must be rejected before ACK");
    assert!(rejection.message().contains("gap"));

    let backpressure = ShippingBackpressureRequest::new(Lsn::new(5), Lsn::new(6));
    assert_eq!(backpressure.replica_received_lsn, Lsn::new(5));
    assert_eq!(backpressure.replica_expected_next_lsn, Lsn::new(6));

    Ok(())
}
