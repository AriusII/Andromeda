use crate::support::*;

#[test]
fn hadr_audit_event_replica_health_transition_is_constructible() {
    let event = HadrAuditEvent::ReplicaHealthTransition {
        replica_id: 42,
        from: ReplicaHealthState::Alive,
        to: ReplicaHealthState::Suspect,
    };

    assert_eq!(event.event_type(), "replica_health_transition");
    assert_eq!(event.affected_replica_id(), Some(42));
}

#[test]
fn hadr_audit_event_fencing_decision_is_constructible() {
    let event = HadrAuditEvent::FencingDecision {
        policy: FencingPolicy::ConservativeQuorum,
        event: FencingEvent::ReplicaSuspect { replica_id: 1 },
        decision: FencingDecision::Block,
    };

    assert_eq!(event.event_type(), "fencing_decision");
    assert_eq!(event.affected_replica_id(), None); // Fencing is not replica-specific
}

#[test]
fn hadr_audit_event_wal_segment_shipped_is_constructible() {
    let event = HadrAuditEvent::WalSegmentShipped {
        segment_id: 100,
        replica_id: 2,
        lsn_range: (1000, 2000),
        checksum: 0xDEADBEEF,
    };

    assert_eq!(event.event_type(), "wal_segment_shipped");
    assert_eq!(event.affected_replica_id(), Some(2));
}

#[test]
fn hadr_audit_event_promotion_eligibility_computed_is_constructible() {
    let event = HadrAuditEvent::PromotionEligibilityComputed {
        replica_id: 3,
        eligibility: PromotionEligibility::Eligible,
    };

    assert_eq!(event.event_type(), "promotion_eligibility_computed");
    assert_eq!(event.affected_replica_id(), Some(3));
}

#[test]
fn hadr_audit_event_promotion_eligibility_ineligible_reasons_are_distinct() {
    let eligible = HadrAuditEvent::PromotionEligibilityComputed {
        replica_id: 1,
        eligibility: PromotionEligibility::Eligible,
    };

    let wal_gap = HadrAuditEvent::PromotionEligibilityComputed {
        replica_id: 1,
        eligibility: PromotionEligibility::WalGapTooLarge,
    };

    assert_ne!(eligible, wal_gap);
}

#[test]
fn hadr_audit_event_promotion_executed_is_constructible() {
    let event = HadrAuditEvent::PromotionExecuted {
        promoted_replica_id: 4,
        new_epoch: 5,
    };

    assert_eq!(event.event_type(), "promotion_executed");
    assert_eq!(event.affected_replica_id(), Some(4));
}

#[test]
fn hadr_audit_event_membership_change_is_constructible() {
    let members = vec![(1, QuorumRole::Primary), (2, QuorumRole::SyncReplica)];
    let event = HadrAuditEvent::MembershipChange {
        old_epoch: 1,
        new_epoch: 2,
        members,
    };

    assert_eq!(event.event_type(), "membership_change");
    assert_eq!(event.affected_replica_id(), None);
}
