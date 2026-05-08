use super::*;
use crate::{Lsn, write_ahead_log::WalGcCandidate};

#[test]
fn reclaimability_decision_reason_messages() {
    assert_eq!(
        ReclaimabilityDecision::Reclaimable.reason(),
        "segment is outside all retention boundaries and can be safely garbage-collected"
    );

    let blocked_recovery = ReclaimabilityDecision::BlockedByRecovery {
        segment_start_lsn: Lsn::new(100),
        required_recovery_lsn: Lsn::new(200),
    };
    assert!(!blocked_recovery.is_reclaimable());
    assert!(blocked_recovery.reason().contains("recovery"));

    let blocked_visibility = ReclaimabilityDecision::BlockedByVisibility {
        segment_end_lsn: Lsn::new(300),
        min_active_snapshot_lsn: Lsn::new(300),
    };
    assert!(blocked_visibility.reason().contains("snapshot"));

    let blocked_replication = ReclaimabilityDecision::BlockedByReplication {
        segment_end_lsn: Lsn::new(400),
        min_standby_received_lsn: Lsn::new(500),
    };
    assert!(blocked_replication.reason().contains("standbys"));

    let blocked_pitr = ReclaimabilityDecision::BlockedByPitrRetention {
        segment_end_lsn: Lsn::new(600),
        pitr_retention_lsn: Lsn::new(700),
    };
    assert!(blocked_pitr.reason().contains("point-in-time recovery"));
}

#[test]
fn retention_boundary_policy_validates_lsn_ordering() {
    let policy =
        RetentionBoundaryPolicy::new(Lsn::new(100), Lsn::new(200), Lsn::new(150), Lsn::new(300));
    assert!(policy.is_ok());

    let err =
        RetentionBoundaryPolicy::new(Lsn::new(300), Lsn::new(200), Lsn::new(150), Lsn::new(400));
    assert!(err.is_err());

    let policy =
        RetentionBoundaryPolicy::new(Lsn::new(100), Lsn::new(400), Lsn::new(150), Lsn::new(300));
    assert!(policy.is_ok());
}

#[test]
fn retention_boundary_policy_gc_boundary_is_minimum() {
    let policy =
        RetentionBoundaryPolicy::new(Lsn::new(100), Lsn::new(300), Lsn::new(250), Lsn::new(500))
            .unwrap();

    assert_eq!(policy.gc_boundary_lsn(), Lsn::new(250));
}

#[test]
fn default_policy_reclaimable_segment() {
    let policy = DefaultReclaimabilityPolicy::with_lsns(
        Lsn::new(100),
        Lsn::new(400),
        Lsn::new(350),
        Lsn::new(500),
    )
    .unwrap();

    let candidate = WalGcCandidate::new(1, Lsn::new(101), Lsn::new(349), 65536).unwrap();
    assert_eq!(
        policy.can_reclaim_segment(&candidate),
        ReclaimabilityDecision::Reclaimable
    );
}

#[test]
fn default_policy_blocked_by_recovery() {
    let policy = DefaultReclaimabilityPolicy::with_lsns(
        Lsn::new(200),
        Lsn::new(400),
        Lsn::new(350),
        Lsn::new(500),
    )
    .unwrap();

    let candidate = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(199), 65536).unwrap();
    assert!(matches!(
        policy.can_reclaim_segment(&candidate),
        ReclaimabilityDecision::BlockedByRecovery { .. }
    ));
}

#[test]
fn default_policy_blocked_by_visibility() {
    let policy = DefaultReclaimabilityPolicy::with_lsns(
        Lsn::new(100),
        Lsn::new(300),
        Lsn::new(350),
        Lsn::new(500),
    )
    .unwrap();

    let candidate = WalGcCandidate::new(1, Lsn::new(200), Lsn::new(350), 65536).unwrap();
    assert!(matches!(
        policy.can_reclaim_segment(&candidate),
        ReclaimabilityDecision::BlockedByVisibility { .. }
    ));
}

#[test]
fn default_policy_blocked_by_replication() {
    let policy = DefaultReclaimabilityPolicy::with_lsns(
        Lsn::new(100),
        Lsn::new(400),
        Lsn::new(250),
        Lsn::new(500),
    )
    .unwrap();

    let candidate = WalGcCandidate::new(1, Lsn::new(200), Lsn::new(300), 65536).unwrap();
    assert!(matches!(
        policy.can_reclaim_segment(&candidate),
        ReclaimabilityDecision::BlockedByReplication { .. }
    ));
}

#[test]
fn default_policy_blocks_by_visibility_before_pitr() {
    let policy = DefaultReclaimabilityPolicy::with_lsns(
        Lsn::new(100),
        Lsn::new(300),
        Lsn::new(350),
        Lsn::new(400),
    )
    .unwrap();

    let candidate = WalGcCandidate::new(1, Lsn::new(200), Lsn::new(350), 65536).unwrap();
    assert!(matches!(
        policy.can_reclaim_segment(&candidate),
        ReclaimabilityDecision::BlockedByVisibility { .. }
    ));
}

#[test]
fn evidence_evaluation_matches_default_policy() {
    let boundaries =
        RetentionBoundaryPolicy::new(Lsn::new(100), Lsn::new(400), Lsn::new(250), Lsn::new(500))
            .unwrap();
    let candidate = WalGcCandidate::new(1, Lsn::new(200), Lsn::new(300), 65536).unwrap();
    let policy = DefaultReclaimabilityPolicy::new(boundaries).unwrap();

    assert_eq!(
        ReclaimabilityEvidence::new(&candidate, boundaries).evaluate(),
        policy.can_reclaim_segment(&candidate)
    );
}

#[test]
fn reclaimability_decision_blocking_lsn() {
    let decision = ReclaimabilityDecision::BlockedByRecovery {
        segment_start_lsn: Lsn::new(100),
        required_recovery_lsn: Lsn::new(200),
    };
    assert_eq!(decision.blocking_lsn(), Some(Lsn::new(200)));

    assert_eq!(ReclaimabilityDecision::Reclaimable.blocking_lsn(), None);
}

#[test]
fn retention_boundary_validate_passes_for_valid_policy() {
    let policy =
        RetentionBoundaryPolicy::new(Lsn::new(100), Lsn::new(200), Lsn::new(150), Lsn::new(300))
            .unwrap();
    assert!(policy.validate().is_ok());
}
