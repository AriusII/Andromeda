use andromeda_hadr::quorum_runtime::{FencingPolicy, ReplicationMode};
use andromeda_hadr::shipping_contract::{
    WalReplicaSafeLsnTracker, WalShipmentRange, WalShippingAck, WalShippingAckBindingRejection,
    WalShippingEvidenceV0,
};
use andromeda_hadr::shipping_runtime::{
    WalShippingRuntimeEvidenceRejection, advance_replica_safe_lsn_from_evidence,
};
use andromeda_wal::Lsn;

fn evidence(replica_id: u64, first: u64, last: u64, ack_lsn: u64) -> WalShippingEvidenceV0 {
    WalShippingEvidenceV0::new(
        1,
        replica_id,
        9,
        WalShipmentRange {
            first: Lsn::new(first),
            last: Lsn::new(last),
            count: (last - first + 1) as usize,
        },
        Lsn::new(last),
        Lsn::new(ack_lsn),
        Lsn::new(ack_lsn),
        ReplicationMode::QuorumEnforced,
        FencingPolicy::BlockOnQuorumLoss,
        0,
        [0xA5; 32],
        None,
    )
    .expect("typed WAL shipping evidence")
}

#[test]
fn shipped_catalog_publication_marks_storage_lsn_boundary_replica_safe() {
    let mut tracker = WalReplicaSafeLsnTracker::new([2]).expect("tracker");
    tracker
        .record_validated_range(
            2,
            WalShipmentRange {
                first: Lsn::new(1),
                last: Lsn::new(20),
                count: 20,
            },
        )
        .expect("catalog publication range validated");
    tracker
        .record_ack(WalShippingAck::new(2, Lsn::new(20)))
        .expect("catalog publication shipped");

    assert!(tracker.all_required_replicas_have_shipped(Lsn::new(20)));
    assert!(!tracker.all_required_replicas_have_shipped(Lsn::new(21)));
}

#[test]
fn runtime_evidence_retention_floor_remains_min_required_safe_lsn() {
    let mut tracker = WalReplicaSafeLsnTracker::new([2, 3]).expect("tracker");

    advance_replica_safe_lsn_from_evidence(&mut tracker, &evidence(2, 1, 20, 20))
        .expect("replica 2 advances");
    assert_eq!(
        tracker.retention_boundary_lsn(),
        Lsn::ZERO,
        "retention floor must not advance past an unadvanced required replica"
    );
    assert!(!tracker.all_required_replicas_have_shipped(Lsn::new(1)));

    assert_eq!(
        advance_replica_safe_lsn_from_evidence(&mut tracker, &evidence(3, 5, 20, 20))
            .expect_err("replica 3 cannot high-jump over unvalidated prefix"),
        WalShippingRuntimeEvidenceRejection::AckBinding(
            WalShippingAckBindingRejection::AckOutsideDurablePrefix,
        )
    );
    assert_eq!(tracker.retention_boundary_lsn(), Lsn::ZERO);

    advance_replica_safe_lsn_from_evidence(&mut tracker, &evidence(3, 1, 20, 20))
        .expect("replica 3 advances after contiguous evidence from current safe LSN");
    assert_eq!(tracker.retention_boundary_lsn(), Lsn::new(20));
    assert!(tracker.all_required_replicas_have_shipped(Lsn::new(20)));
}
