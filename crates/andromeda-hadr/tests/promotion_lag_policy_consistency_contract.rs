use andromeda_hadr::quorum_runtime::{
    QuorumMembership, QuorumMembershipRejection, ReplicaHealthState, ReplicaMember,
    select_promotion_candidate,
};
use andromeda_wal::Lsn;

#[test]
fn promotion_eligibility_requires_replica_safe_lsn_at_or_ahead_of_primary() {
    let primary_durable = Lsn::new(1_000);
    let mut lagging = ReplicaMember::new(2, Lsn::new(901), Lsn::new(1_000));
    lagging.health_state = ReplicaHealthState::Alive;
    assert!(!lagging.is_promotion_eligible(primary_durable));

    let mut almost_caught_up = ReplicaMember::new(3, Lsn::new(999), Lsn::new(1_000));
    almost_caught_up.health_state = ReplicaHealthState::Alive;
    assert!(!almost_caught_up.is_promotion_eligible(primary_durable));

    let mut caught_up = ReplicaMember::new(4, Lsn::new(1_000), Lsn::new(1_000));
    caught_up.health_state = ReplicaHealthState::Alive;
    assert!(caught_up.is_promotion_eligible(primary_durable));

    let mut ahead = ReplicaMember::new(5, Lsn::new(1_100), Lsn::new(1_100));
    ahead.health_state = ReplicaHealthState::Alive;
    assert!(ahead.is_promotion_eligible(primary_durable));
}

#[test]
fn promotion_selection_does_not_pick_lagging_replica_within_legacy_sub_100_window()
-> Result<(), QuorumMembershipRejection> {
    let replicas = vec![
        ReplicaMember::new(2, Lsn::new(950), Lsn::new(1_000)),
        ReplicaMember::new(3, Lsn::new(999), Lsn::new(1_000)),
    ];
    let membership = QuorumMembership::new(1, replicas)?;

    assert_eq!(
        select_promotion_candidate(&membership, Lsn::new(1_000)),
        None,
        "strict policy must reject lagging replicas even if lag < 100"
    );
    Ok(())
}
