use andromeda_storage::{
    Lsn,
    write_ahead_log::{WalReplicaSafeLsnTracker, WalShippingAck},
};

#[test]
fn shipped_catalog_publication_marks_storage_lsn_boundary_replica_safe() {
    let mut tracker = WalReplicaSafeLsnTracker::new([2]).expect("tracker");
    tracker
        .record_ack(WalShippingAck::new(2, Lsn::new(20)))
        .expect("catalog publication shipped");

    assert!(tracker.all_required_replicas_have_shipped(Lsn::new(20)));
    assert!(!tracker.all_required_replicas_have_shipped(Lsn::new(21)));
}
