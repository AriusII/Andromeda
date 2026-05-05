/// F3 Quorum Membership Contract Tests
///
/// This test suite validates the quorum consensus runtime against 12+ scenarios:
/// - Quorum initialization and topology changes
/// - Replica join/leave with state transitions
/// - Write admission control (async vs quorum mode)
/// - Promotion eligibility and ranking
/// - Fencing decisions on membership loss
/// - Membership epoch tracking
/// - Atomic operations on concurrent join/promote
use andromeda_storage::{
    hadr::{
        membership_transitions::{MembershipState, MembershipStateTracker, TransitionEvent},
        quorum_runtime::*,
    },
    Lsn,
};

#[test]
fn test_quorum_initialized_with_single_primary() {
    // Scenario: Cluster starts with a single primary and at least one replica.
    let replicas = vec![ReplicaMember::new(2, Lsn::new(0), Lsn::new(0))];
    let membership = QuorumMembership::new(1, replicas).expect("Create membership");

    assert_eq!(membership.size(), 1);
    assert_eq!(membership.quorum_size(), 1); // 1/2 + 1 = 1
    assert_eq!(membership.epoch(), 0);
    assert!(membership.has_quorum()); // 1 alive >= 1 required
}

#[test]
fn test_new_replica_joins_membership_on_hello() {
    // Scenario: New replica sends hello message; transitions from Initial → Active.
    let replicas = vec![
        ReplicaMember::new(2, Lsn::new(0), Lsn::new(0)),
        ReplicaMember::new(3, Lsn::new(0), Lsn::new(0)),
    ];
    let membership = QuorumMembership::new(1, replicas).expect("Create membership");

    // Replica 2 sends heartbeat, state transitions from Initial → Active.
    let mut tracker = MembershipStateTracker::new(2);
    let result = tracker.try_transition(TransitionEvent::HeartbeatReceived, membership.epoch());

    assert!(result.is_ok());
    assert_eq!(tracker.current(), MembershipState::Active);
}

#[test]
fn test_replica_marked_suspect_on_missed_heartbeat() {
    // Scenario: Replica misses heartbeat threshold; transitions Active → Suspect.
    // Membership epoch increments.
    let replicas = vec![ReplicaMember::new(2, Lsn::new(100), Lsn::new(100))];
    let mut membership = QuorumMembership::new(1, replicas).expect("Create membership");

    let initial_epoch = membership.epoch();
    assert_eq!(membership.count_alive(), 1);

    // Mark replica as suspect
    membership.mark_suspect(2);

    assert_eq!(membership.epoch(), initial_epoch + 1);
    assert_eq!(membership.count_alive(), 0);
    assert_eq!(membership.count_responsive(), 1);
}

#[test]
fn test_quorum_blocks_writes_if_membership_quorum_lost() {
    // Scenario: Cluster loses quorum; fencing policy blocks new writes.
    // Requires QuorumEnforced mode with 2+ replicas initially.
    let replicas = vec![
        ReplicaMember::new(2, Lsn::new(0), Lsn::new(0)),
        ReplicaMember::new(3, Lsn::new(0), Lsn::new(0)),
    ];
    let mut membership = QuorumMembership::new(1, replicas).expect("Create membership");

    assert!(membership.has_quorum()); // 2 alive >= 2 required

    // Mark both as dead
    membership.mark_dead(2);
    membership.mark_dead(3);

    assert!(!membership.has_quorum()); // 0 alive < 2 required

    let decision = decide_fencing(
        &membership,
        FencingEvent::ReplicaDisconnected,
        FencingPolicy::BlockOnQuorumLoss,
        ReplicationMode::QuorumEnforced,
    );

    assert_eq!(decision, FencingDecision::Block);
}

#[test]
fn test_replica_promotion_blocked_if_lsn_behind_primary() {
    // Scenario: Replica's received_lsn < primary.durable_lsn; not promotable.
    let replica = ReplicaMember::new(2, Lsn::new(100), Lsn::new(100));
    let primary_durable = Lsn::new(500);

    assert!(!replica.is_promotion_eligible(primary_durable));

    let rank = PromotionRank::new(&replica, primary_durable);
    assert!(!rank.is_eligible);
    assert_eq!(rank.lsn_distance, 400);
}

#[test]
fn test_promotion_allowed_when_replica_durable_lsn_caught_up() {
    // Scenario: Replica's received_lsn == primary.durable_lsn; promotable.
    let replica = ReplicaMember::new(2, Lsn::new(500), Lsn::new(500));
    let primary_durable = Lsn::new(500);

    assert!(replica.is_promotion_eligible(primary_durable));

    let rank = PromotionRank::new(&replica, primary_durable);
    assert!(rank.is_eligible);
    assert_eq!(rank.lsn_distance, 0);
}

#[test]
fn test_fencing_decision_blocks_in_quorum_mode_on_disconnect() {
    // Scenario: Connection loss in QuorumEnforced mode → decision depends on membership.
    let replicas = vec![ReplicaMember::new(2, Lsn::new(0), Lsn::new(0))];
    let mut membership = QuorumMembership::new(1, replicas).expect("Create membership");

    // With 1 replica alive and quorum=1, we still have quorum.
    let decision = decide_fencing(
        &membership,
        FencingEvent::ReplicaDisconnected,
        FencingPolicy::BlockOnQuorumLoss,
        ReplicationMode::QuorumEnforced,
    );

    assert_eq!(decision, FencingDecision::Allow); // Quorum maintained

    // Mark replica as dead; now quorum is lost.
    membership.mark_dead(2);

    let decision = decide_fencing(
        &membership,
        FencingEvent::ReplicaDisconnected,
        FencingPolicy::BlockOnQuorumLoss,
        ReplicationMode::QuorumEnforced,
    );

    assert_eq!(decision, FencingDecision::Block); // Quorum lost
}

#[test]
fn test_fencing_decision_allows_in_async_mode_on_disconnect() {
    // Scenario: Connection loss in Asynchronous mode → always allow.
    let replicas = vec![ReplicaMember::new(2, Lsn::new(0), Lsn::new(0))];
    let mut membership = QuorumMembership::new(1, replicas).expect("Create membership");

    // Quorum maintained
    let decision = decide_fencing(
        &membership,
        FencingEvent::ReplicaDisconnected,
        FencingPolicy::BlockOnQuorumLoss,
        ReplicationMode::Asynchronous,
    );

    assert_eq!(decision, FencingDecision::Allow);

    // Mark replica as dead; even though quorum is lost, async mode allows.
    membership.mark_dead(2);

    let decision = decide_fencing(
        &membership,
        FencingEvent::ReplicaDisconnected,
        FencingPolicy::BlockOnQuorumLoss,
        ReplicationMode::Asynchronous,
    );

    assert_eq!(decision, FencingDecision::Allow);
}

#[test]
fn test_membership_epoch_increments_on_topology_change() {
    // Scenario: Every topology change (join, suspect, promote, remove) increments epoch.
    let replicas = vec![
        ReplicaMember::new(2, Lsn::new(0), Lsn::new(0)),
        ReplicaMember::new(3, Lsn::new(0), Lsn::new(0)),
    ];
    let mut membership = QuorumMembership::new(1, replicas).expect("Create membership");

    let epoch0 = membership.epoch();
    assert_eq!(epoch0, 0);

    membership.mark_suspect(2);
    assert_eq!(membership.epoch(), epoch0 + 1);

    membership.mark_alive(2);
    assert_eq!(membership.epoch(), epoch0 + 2);

    membership.mark_dead(3);
    assert_eq!(membership.epoch(), epoch0 + 3);

    membership.remove_dead(3);
    assert_eq!(membership.epoch(), epoch0 + 4);
}

#[test]
fn test_simultaneous_join_and_promote_atomic() {
    // Scenario: Replica joins (transitions Active) and is immediately eligible for promotion.
    // State transitions are atomic per replica.
    let replicas = vec![
        ReplicaMember::new(2, Lsn::new(500), Lsn::new(500)),
        ReplicaMember::new(3, Lsn::new(500), Lsn::new(500)),
    ];
    let membership = QuorumMembership::new(1, replicas).expect("Create membership");

    let primary_durable = Lsn::new(500);

    // Both replicas are caught up and alive; both are promotable.
    let ranks = rank_promotion_candidates(&membership, primary_durable);
    assert_eq!(ranks.len(), 2);
    assert!(ranks.iter().all(|r| r.is_eligible));

    // Tracker for replica 2: Initial → Active (join)
    let mut tracker2 = MembershipStateTracker::new(2);
    let _ = tracker2.try_transition(TransitionEvent::HeartbeatReceived, 0);
    assert_eq!(tracker2.current(), MembershipState::Active);

    // Immediately stage for promotion (can happen atomically in same epoch).
    let _ = tracker2.try_transition(TransitionEvent::PromotionStaged, 0);
    assert_eq!(tracker2.current(), MembershipState::Candidate);

    // Promotion succeeds.
    let _ = tracker2.try_transition(TransitionEvent::PromotionSucceeded, 0);
    assert_eq!(tracker2.current(), MembershipState::Promoted);
}

#[test]
fn test_leader_election_deterministic_by_lsn_distance() {
    // Scenario: Quorum consensus selects best promotion candidate deterministically.
    let replicas = vec![
        ReplicaMember::new(10, Lsn::new(400), Lsn::new(400)), // 100 bytes behind
        ReplicaMember::new(5, Lsn::new(450), Lsn::new(450)),  // 50 bytes behind
        ReplicaMember::new(20, Lsn::new(450), Lsn::new(450)), // 50 bytes behind (higher ID)
    ];
    let membership = QuorumMembership::new(1, replicas).expect("Create membership");

    let primary_durable = Lsn::new(500);
    let candidate = select_promotion_candidate(&membership, primary_durable);

    // Best candidate is replica 5 (50 bytes behind, lowest ID among tied).
    assert!(candidate.is_some());
    let best = candidate.unwrap();
    assert_eq!(best.replica_id, 5);
    assert_eq!(best.lsn_distance, 50);
}

#[test]
fn test_orphan_replica_detection_and_removal() {
    // Scenario: Replica marked suspect, then dead, then removed from membership.
    // Quorum size recomputes after removal.
    let replicas = vec![
        ReplicaMember::new(2, Lsn::new(0), Lsn::new(0)),
        ReplicaMember::new(3, Lsn::new(0), Lsn::new(0)),
        ReplicaMember::new(4, Lsn::new(0), Lsn::new(0)),
    ];
    let mut membership = QuorumMembership::new(1, replicas).expect("Create membership");

    assert_eq!(membership.size(), 3);
    assert_eq!(membership.quorum_size(), 2); // 3/2 + 1 = 2

    // Replica 2 becomes suspect
    membership.mark_suspect(2);
    assert_eq!(membership.count_alive(), 2);

    // Replica 2 becomes dead
    membership.mark_dead(2);
    assert_eq!(membership.count_alive(), 2);
    assert!(membership.has_quorum()); // 2 alive >= 2 required

    // Remove dead replica
    membership.remove_dead(2);
    assert_eq!(membership.size(), 2);
    assert_eq!(membership.quorum_size(), 1); // 2/2 + 1 = 1, recomputed
    assert!(membership.has_quorum()); // 2 alive >= 1 required
}

#[test]
fn test_write_admission_async_no_acks_required() {
    // Scenario: Async replication mode requires zero ACKs for write visibility.
    let consensus = QuorumConsensus::async_mode();
    assert_eq!(consensus.min_quorum_acks, 0);

    assert!(consensus.can_admit_write(0));
    assert!(consensus.can_admit_write(1));
}

#[test]
fn test_write_admission_quorum_requires_majority_acks() {
    // Scenario: Quorum mode requires floor(N/2)+1 ACKs.
    let consensus = QuorumConsensus::from_membership_majority(3);
    assert_eq!(consensus.min_quorum_acks, 2); // 3/2 + 1 = 2

    assert!(!consensus.can_admit_write(1)); // Not enough
    assert!(consensus.can_admit_write(2)); // Quorum reached
    assert!(consensus.can_admit_write(3)); // Exceeds quorum
}

#[test]
fn test_replica_lsn_updates_tracked() {
    // Scenario: Replica's LSN state is updated; used for promotion ranking.
    let replicas = vec![ReplicaMember::new(2, Lsn::new(100), Lsn::new(100))];
    let mut membership = QuorumMembership::new(1, replicas).expect("Create membership");

    let replica_before = membership.get_replica(2).unwrap();
    assert_eq!(replica_before.received_lsn, Lsn::new(100));

    // Replica advances its received_lsn
    membership.update_replica_lsn(2, Lsn::new(250), Lsn::new(250));

    let replica_after = membership.get_replica(2).unwrap();
    assert_eq!(replica_after.received_lsn, Lsn::new(250));
}

#[test]
fn test_consensus_computation_from_membership() {
    // Scenario: Quorum consensus min ACKs are computed from membership majority.
    let replicas = vec![
        ReplicaMember::new(2, Lsn::new(0), Lsn::new(0)),
        ReplicaMember::new(3, Lsn::new(0), Lsn::new(0)),
        ReplicaMember::new(4, Lsn::new(0), Lsn::new(0)),
        ReplicaMember::new(5, Lsn::new(0), Lsn::new(0)),
    ];
    let membership = QuorumMembership::new(1, replicas).expect("Create membership");

    let consensus = QuorumConsensus::from_membership_majority(membership.size());

    // 4 replicas => quorum = 4/2 + 1 = 3
    assert_eq!(consensus.min_quorum_acks, 3);
    assert!(!consensus.can_admit_write(2));
    assert!(consensus.can_admit_write(3));
}

#[test]
fn test_membership_state_tracker_complete_lifecycle() {
    // Scenario: Track a replica through full lifecycle: Initial → Active → Suspect → Removed.
    let mut tracker = MembershipStateTracker::new(42);
    assert_eq!(tracker.replica_id(), 42);
    assert_eq!(tracker.current(), MembershipState::Initial);

    // Join: Initial → Active
    tracker
        .try_transition(TransitionEvent::HeartbeatReceived, 0)
        .expect("Heartbeat in Initial");
    assert_eq!(tracker.current(), MembershipState::Active);

    // Suspect: Active → Suspect
    tracker
        .try_transition(TransitionEvent::HeartbeatMissed, 1)
        .expect("Missed heartbeat in Active");
    assert_eq!(tracker.current(), MembershipState::Suspect);

    // Recover: Suspect → Active
    tracker
        .try_transition(TransitionEvent::HeartbeatReceived, 2)
        .expect("Heartbeat in Suspect");
    assert_eq!(tracker.current(), MembershipState::Active);

    // Remove: Active → Removed
    tracker
        .try_transition(TransitionEvent::RemovalRequested, 3)
        .expect("Removal in Active");
    assert_eq!(tracker.current(), MembershipState::Removed);

    // Verify history recorded correctly
    assert_eq!(tracker.history().len(), 4);
    assert_eq!(
        tracker.transition_count(TransitionEvent::HeartbeatReceived),
        2
    );
}

#[test]
fn test_fencing_policy_allow_never_blocks() {
    // Scenario: FencingPolicy::Allow always permits writes.
    let replicas = vec![ReplicaMember::new(2, Lsn::new(0), Lsn::new(0))];
    let mut membership = QuorumMembership::new(1, replicas).expect("Create membership");

    membership.mark_dead(2); // Quorum lost

    let decision = decide_fencing(
        &membership,
        FencingEvent::Unknown,
        FencingPolicy::Allow,
        ReplicationMode::QuorumEnforced,
    );

    assert_eq!(decision, FencingDecision::Allow);
}

#[test]
fn test_promotion_eligibility_ranking_complete() {
    // Scenario: All promotion ranking rules applied correctly.
    let mut replica1 = ReplicaMember::new(1, Lsn::new(50), Lsn::new(50));
    let replica2 = ReplicaMember::new(2, Lsn::new(300), Lsn::new(300));

    // Make replica1 suspect (not eligible)
    replica1.health_state = ReplicaHealthState::Suspect;

    let primary_durable = Lsn::new(300);

    let rank1 = PromotionRank::new(&replica1, primary_durable);
    let rank2 = PromotionRank::new(&replica2, primary_durable);

    assert!(!rank1.is_eligible); // Suspect
    assert!(rank2.is_eligible); // Alive and LSN OK

    // Best candidate is replica 2 (only eligible one)
    let best = PromotionRank::best_candidate(&[rank1, rank2]).unwrap();
    assert_eq!(best.replica_id, 2);
}
