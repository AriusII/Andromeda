use andromeda_core::{CatalogObjectId, CatalogVersion, ContractHash};
use andromeda_storage::{
    CatalogWalRecord, Lsn, LsnBoundCatalogRecord, replay_catalog_from_lsn, write_ahead_log::*,
};

#[test]
fn shipping_ack_advances_replica_safe_lsn() {
    let mut tracker = WalReplicaSafeLsnTracker::new([2, 3]).expect("tracker");

    assert_eq!(
        tracker.record_ack(WalShippingAck::new(2, Lsn::new(100))),
        Ok(Lsn::new(100))
    );
    assert_eq!(
        tracker.record_ack(WalShippingAck::new(2, Lsn::new(90))),
        Ok(Lsn::new(100))
    );
    assert_eq!(tracker.replica_safe_lsn(2), Some(Lsn::new(100)));
    assert!(
        tracker
            .record_ack(WalShippingAck::new(9, Lsn::new(100)))
            .is_err()
    );
}

#[test]
fn segment_not_reclaimed_until_required_replicas_shipped() {
    let mut tracker = WalReplicaSafeLsnTracker::new([2, 3]).expect("tracker");
    tracker
        .record_ack(WalShippingAck::new(2, Lsn::new(300)))
        .expect("replica 2 ack");
    tracker
        .record_ack(WalShippingAck::new(3, Lsn::new(199)))
        .expect("replica 3 lagging");

    let segment = WalGcCandidate::new(7, Lsn::new(200), Lsn::new(300), 65536).unwrap();
    let blocked = DefaultReclaimabilityPolicy::with_replica_safe_lsn_tracker(
        Lsn::new(50),
        Lsn::new(500),
        &tracker,
        Lsn::new(500),
    )
    .unwrap();

    assert!(matches!(
        blocked.can_reclaim_segment(&segment),
        ReclaimabilityDecision::BlockedByReplication { .. }
    ));
    assert!(!tracker.all_required_replicas_have_shipped(segment.sealing_lsn));

    tracker
        .record_ack(WalShippingAck::new(3, Lsn::new(300)))
        .expect("replica 3 catches up");
    let reclaimable = DefaultReclaimabilityPolicy::with_replica_safe_lsn_tracker(
        Lsn::new(50),
        Lsn::new(500),
        &tracker,
        Lsn::new(500),
    )
    .unwrap();

    assert_eq!(
        reclaimable.can_reclaim_segment(&segment),
        ReclaimabilityDecision::Reclaimable
    );
    assert!(tracker.all_required_replicas_have_shipped(segment.sealing_lsn));
}

#[test]
fn required_replica_without_ack_blocks_segment_reclaim() {
    let mut tracker = WalReplicaSafeLsnTracker::new([2, 3]).expect("tracker");
    tracker
        .record_ack(WalShippingAck::new(2, Lsn::new(300)))
        .expect("replica 2 ack");

    let segment = WalGcCandidate::new(8, Lsn::new(200), Lsn::new(300), 65536).unwrap();
    let policy = DefaultReclaimabilityPolicy::with_replica_safe_lsn_tracker(
        Lsn::new(50),
        Lsn::new(500),
        &tracker,
        Lsn::new(500),
    )
    .unwrap();

    assert_eq!(tracker.replica_safe_lsn(3), Some(Lsn::ZERO));
    assert!(matches!(
        policy.can_reclaim_segment(&segment),
        ReclaimabilityDecision::BlockedByReplication {
            segment_end_lsn,
            min_standby_received_lsn,
        } if segment_end_lsn == Lsn::new(300) && min_standby_received_lsn == Lsn::ZERO
    ));
}

#[test]
fn replica_catchup_replays_catalog_publication() {
    let mut tracker = WalReplicaSafeLsnTracker::new([2]).expect("tracker");
    tracker
        .record_ack(WalShippingAck::new(2, Lsn::new(20)))
        .expect("catalog publication shipped");

    let lsn_records = vec![LsnBoundCatalogRecord::new(
        Lsn::new(20),
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(42),
            signature_hash: ContractHash::test_vector(0x42),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1_700_000_000,
        },
    )];

    assert!(tracker.all_required_replicas_have_shipped(Lsn::new(20)));
    let replay = replay_catalog_from_lsn(&lsn_records, Lsn::new(20), CatalogVersion::new(1))
        .expect("catalog replay");

    assert_eq!(replay.records_replayed, 1);
    assert_eq!(replay.snapshot.catalog_version, CatalogVersion::new(1));
    assert!(
        replay
            .snapshot
            .procedure_ids
            .contains(&CatalogObjectId::new(42))
    );
}
