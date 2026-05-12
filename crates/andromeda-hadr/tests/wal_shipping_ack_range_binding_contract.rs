use andromeda_hadr::quorum_runtime::{FencingPolicy, ReplicationMode};
use andromeda_hadr::shipping_contract::{
    WalReplicaSafeLsnTracker, WalShipmentRange, WalShippingAck, WalShippingAckBindingRejection,
    WalShippingEvidenceRejectionReason, WalShippingEvidenceV0,
};
use andromeda_hadr::shipping_runtime::{
    WalShippingRuntimeEvidenceRejection, advance_replica_safe_lsn_from_evidence,
};
use andromeda_wal::Lsn;

fn range(first: u64, last: u64) -> WalShipmentRange {
    WalShipmentRange {
        first: Lsn::new(first),
        last: Lsn::new(last),
        count: (last - first + 1) as usize,
    }
}

fn evidence(
    replica_id: u64,
    first: u64,
    last: u64,
    ack_lsn: u64,
    rejection_reason: Option<WalShippingEvidenceRejectionReason>,
) -> WalShippingEvidenceV0 {
    WalShippingEvidenceV0::new(
        1,
        replica_id,
        9,
        range(first, last),
        Lsn::new(last),
        Lsn::new(ack_lsn),
        Lsn::new(ack_lsn),
        ReplicationMode::QuorumEnforced,
        FencingPolicy::BlockOnQuorumLoss,
        0,
        [0x5A; 32],
        rejection_reason,
    )
    .expect("typed WAL shipping evidence")
}

#[test]
fn ack_is_bound_to_contiguous_validated_durable_prefix() {
    let mut tracker = WalReplicaSafeLsnTracker::new([2]).expect("tracker");

    tracker
        .record_validated_range(2, range(1, 3))
        .expect("prefix");
    tracker
        .record_validated_range(2, range(5, 6))
        .expect("out-of-order gap segment");

    tracker
        .record_ack_bound(WalShippingAck::new(2, Lsn::new(3)))
        .expect("ack at contiguous prefix end");

    assert_eq!(
        tracker
            .record_ack_bound(WalShippingAck::new(2, Lsn::new(4)))
            .expect_err("gap not validated"),
        WalShippingAckBindingRejection::AckOutsideDurablePrefix
    );
    assert_eq!(
        tracker
            .record_ack_bound(WalShippingAck::new(2, Lsn::new(7)))
            .expect_err("ack above shipped max"),
        WalShippingAckBindingRejection::AckExceedsShippedMax
    );

    tracker
        .record_validated_range(2, range(4, 4))
        .expect("bridge gap");
    tracker
        .record_ack_bound(WalShippingAck::new(2, Lsn::new(6)))
        .expect("prefix extends after gap is bridged");

    assert_eq!(
        tracker
            .record_ack_bound(WalShippingAck::new(2, Lsn::new(6)))
            .expect_err("stale ack"),
        WalShippingAckBindingRejection::StaleAck
    );
}

#[test]
fn ack_binding_rejects_invalid_ranges_unknown_replicas_and_reordered_bridgeing() {
    let mut tracker = WalReplicaSafeLsnTracker::new([2]).expect("tracker");

    assert_eq!(
        tracker
            .record_validated_range(
                2,
                WalShipmentRange {
                    first: Lsn::new(50),
                    last: Lsn::new(52),
                    count: 2,
                },
            )
            .expect_err("range count/span mismatch"),
        WalShippingAckBindingRejection::InvalidValidatedRange
    );

    assert_eq!(
        tracker
            .record_ack_bound(WalShippingAck::new(3, Lsn::new(1)))
            .expect_err("ack from non-required replica"),
        WalShippingAckBindingRejection::ReplicaNotRequired
    );

    tracker
        .record_validated_range(2, range(1, 3))
        .expect("range A");
    tracker
        .record_validated_range(2, range(5, 6))
        .expect("range C");
    assert_eq!(
        tracker
            .record_ack_bound(WalShippingAck::new(2, Lsn::new(5)))
            .expect_err("gap blocks ack"),
        WalShippingAckBindingRejection::AckOutsideDurablePrefix
    );

    tracker
        .record_validated_range(2, range(4, 4))
        .expect("bridge B");
    tracker
        .record_ack_bound(WalShippingAck::new(2, Lsn::new(6)))
        .expect("bridged reordered ranges produce contiguous prefix");
}

#[test]
fn ack_binding_rejects_jump_without_validated_prefix_from_current_safe_lsn() {
    let mut tracker = WalReplicaSafeLsnTracker::new([2]).expect("tracker");
    tracker
        .record_validated_range(2, range(100, 100))
        .expect("high isolated range");

    assert_eq!(
        tracker
            .record_ack_bound(WalShippingAck::new(2, Lsn::new(100)))
            .expect_err("ack must not jump over unvalidated prefix"),
        WalShippingAckBindingRejection::AckOutsideDurablePrefix
    );

    tracker
        .record_validated_range(2, range(1, 99))
        .expect("prefix bridge");
    tracker
        .record_ack_bound(WalShippingAck::new(2, Lsn::new(100)))
        .expect("ack is valid once prefix from current safe lsn is complete");
}

#[test]
fn runtime_evidence_advances_tracker_safe_lsn_only_after_binding() {
    let mut tracker = WalReplicaSafeLsnTracker::new([2]).expect("tracker");

    let advanced =
        advance_replica_safe_lsn_from_evidence(&mut tracker, &evidence(2, 1, 4, 4, None))
            .expect("validated contiguous evidence advances safe lsn");

    assert_eq!(advanced, Lsn::new(4));
    assert_eq!(tracker.replica_safe_lsn(2), Some(Lsn::new(4)));
    assert_eq!(tracker.retention_boundary_lsn(), Lsn::new(4));
}

#[test]
fn runtime_evidence_rejects_unvalidated_ack_and_high_jump() {
    let mut tracker = WalReplicaSafeLsnTracker::new([2]).expect("tracker");

    assert_eq!(
        advance_replica_safe_lsn_from_evidence(&mut tracker, &evidence(2, 100, 100, 100, None))
            .expect_err("high-jump ACK must not advance without prefix from current safe LSN"),
        WalShippingRuntimeEvidenceRejection::AckBinding(
            WalShippingAckBindingRejection::AckOutsideDurablePrefix
        )
    );
    assert_eq!(tracker.replica_safe_lsn(2), Some(Lsn::ZERO));
    tracker
        .record_validated_range(2, range(1, 99))
        .expect("prefix without rejected high-jump evidence");
    assert_eq!(
        tracker
            .record_ack_bound(WalShippingAck::new(2, Lsn::new(100)))
            .expect_err("failed high-jump evidence must not leave an orphan range"),
        WalShippingAckBindingRejection::AckExceedsShippedMax
    );

    assert_eq!(
        advance_replica_safe_lsn_from_evidence(
            &mut tracker,
            &evidence(
                2,
                1,
                1,
                1,
                Some(WalShippingEvidenceRejectionReason::AckOutsideDurablePrefix),
            ),
        )
        .expect_err("negative replica evidence must fail closed"),
        WalShippingRuntimeEvidenceRejection::EvidenceRejected(
            WalShippingEvidenceRejectionReason::AckOutsideDurablePrefix
        )
    );
    assert_eq!(tracker.replica_safe_lsn(2), Some(Lsn::ZERO));
}
