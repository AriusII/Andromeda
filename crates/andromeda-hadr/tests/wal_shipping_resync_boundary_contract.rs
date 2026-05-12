use andromeda_hadr::shipping_contract::{
    ReplicaPromotionBlocker, ReplicaPromotionEligibility, ReplicaResyncEvidence,
    ResyncCompatibility, ResyncLagStatus, WalShipmentRange,
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
    snapshot_base_lsn: Option<u64>,
    wal_range: Option<WalShipmentRange>,
    retention_floor_lsn: u64,
    replica_safe_lsn: u64,
    primary_tip_lsn: u64,
    compatibility: ResyncCompatibility,
) -> ReplicaResyncEvidence {
    ReplicaResyncEvidence::new(
        snapshot_base_lsn.map(Lsn::new),
        wal_range,
        Lsn::new(retention_floor_lsn),
        Lsn::new(replica_safe_lsn),
        Lsn::new(primary_tip_lsn),
        0,
        compatibility,
    )
}

fn evidence_with_lag_limit(
    snapshot_base_lsn: Option<u64>,
    wal_range: Option<WalShipmentRange>,
    retention_floor_lsn: u64,
    replica_safe_lsn: u64,
    primary_tip_lsn: u64,
    max_promotion_lag_lsn: u64,
    compatibility: ResyncCompatibility,
) -> ReplicaResyncEvidence {
    ReplicaResyncEvidence::new(
        snapshot_base_lsn.map(Lsn::new),
        wal_range,
        Lsn::new(retention_floor_lsn),
        Lsn::new(replica_safe_lsn),
        Lsn::new(primary_tip_lsn),
        max_promotion_lag_lsn,
        compatibility,
    )
}

#[test]
fn missing_snapshot_blocks_resync_and_promotion() {
    let decision = evidence(
        None,
        Some(range(11, 20)),
        1,
        20,
        20,
        ResyncCompatibility::Compatible,
    )
    .decide();

    assert_eq!(decision.snapshot_base_lsn, None);
    assert_eq!(decision.required_wal_range, None);
    assert_eq!(decision.lag_status, ResyncLagStatus::MissingSnapshot);
    assert_eq!(
        decision.promotion_eligibility,
        ReplicaPromotionEligibility::Blocked(ReplicaPromotionBlocker::MissingSnapshot)
    );
}

#[test]
fn wal_gap_blocks_when_observed_range_does_not_cover_snapshot_to_tip() {
    let decision = evidence(
        Some(10),
        Some(range(12, 20)),
        1,
        20,
        20,
        ResyncCompatibility::Compatible,
    )
    .decide();

    assert_eq!(decision.required_wal_range, Some(range(11, 20)));
    assert_eq!(decision.lag_status, ResyncLagStatus::WalGap);
    assert_eq!(
        decision.promotion_eligibility,
        ReplicaPromotionEligibility::Blocked(ReplicaPromotionBlocker::WalGap)
    );
}

#[test]
fn retention_floor_violation_blocks_when_required_wal_was_reclaimed() {
    let decision = evidence(
        Some(10),
        Some(range(11, 20)),
        12,
        20,
        20,
        ResyncCompatibility::Compatible,
    )
    .decide();

    assert_eq!(decision.required_wal_range, Some(range(11, 20)));
    assert_eq!(
        decision.lag_status,
        ResyncLagStatus::RetentionFloorViolation
    );
    assert_eq!(
        decision.promotion_eligibility,
        ReplicaPromotionEligibility::Blocked(ReplicaPromotionBlocker::RetentionFloorViolation)
    );
}

#[test]
fn promotion_is_blocked_until_local_compatibility_evidence_matches() {
    let incompatible = evidence(
        Some(10),
        Some(range(11, 20)),
        1,
        20,
        20,
        ResyncCompatibility::Incompatible,
    )
    .decide();

    assert_eq!(incompatible.lag_status, ResyncLagStatus::CaughtUp);
    assert_eq!(
        incompatible.promotion_eligibility,
        ReplicaPromotionEligibility::Blocked(ReplicaPromotionBlocker::IncompatibleReplica)
    );

    let compatible = evidence(
        Some(10),
        Some(range(11, 20)),
        1,
        20,
        20,
        ResyncCompatibility::Compatible,
    )
    .decide();

    assert_eq!(compatible.lag_status, ResyncLagStatus::CaughtUp);
    assert_eq!(
        compatible.promotion_eligibility,
        ReplicaPromotionEligibility::Eligible
    );
}

#[test]
fn non_zero_max_promotion_lag_under_threshold_is_eligible() {
    let decision = evidence_with_lag_limit(
        Some(10),
        Some(range(11, 20)),
        1,
        18,
        20,
        3,
        ResyncCompatibility::Compatible,
    )
    .decide();

    assert_eq!(decision.lag_status, ResyncLagStatus::Lagging { lag_lsn: 2 });
    assert_eq!(
        decision.promotion_eligibility,
        ReplicaPromotionEligibility::Eligible
    );
}

#[test]
fn non_zero_max_promotion_lag_equal_threshold_is_eligible() {
    let decision = evidence_with_lag_limit(
        Some(10),
        Some(range(11, 20)),
        1,
        17,
        20,
        3,
        ResyncCompatibility::Compatible,
    )
    .decide();

    assert_eq!(decision.lag_status, ResyncLagStatus::Lagging { lag_lsn: 3 });
    assert_eq!(
        decision.promotion_eligibility,
        ReplicaPromotionEligibility::Eligible
    );
}

#[test]
fn non_zero_max_promotion_lag_over_threshold_is_blocked() {
    let decision = evidence_with_lag_limit(
        Some(10),
        Some(range(11, 20)),
        1,
        16,
        20,
        3,
        ResyncCompatibility::Compatible,
    )
    .decide();

    assert_eq!(decision.lag_status, ResyncLagStatus::Lagging { lag_lsn: 4 });
    assert_eq!(
        decision.promotion_eligibility,
        ReplicaPromotionEligibility::Blocked(ReplicaPromotionBlocker::ReplicaLagging)
    );
}

#[test]
fn malformed_wal_range_zero_count_fails_closed_with_invalid_range_blocker() {
    let decision = evidence(
        Some(10),
        Some(WalShipmentRange {
            first: Lsn::new(11),
            last: Lsn::new(20),
            count: 0,
        }),
        1,
        20,
        20,
        ResyncCompatibility::Compatible,
    )
    .decide();

    assert_eq!(decision.lag_status, ResyncLagStatus::WalGap);
    assert_eq!(
        decision.promotion_eligibility,
        ReplicaPromotionEligibility::Blocked(ReplicaPromotionBlocker::InvalidWalRange)
    );
}

#[test]
fn malformed_wal_range_backwards_fails_closed_with_invalid_range_blocker() {
    let decision = evidence(
        Some(10),
        Some(WalShipmentRange {
            first: Lsn::new(20),
            last: Lsn::new(11),
            count: 10,
        }),
        1,
        20,
        20,
        ResyncCompatibility::Compatible,
    )
    .decide();

    assert_eq!(decision.lag_status, ResyncLagStatus::WalGap);
    assert_eq!(
        decision.promotion_eligibility,
        ReplicaPromotionEligibility::Blocked(ReplicaPromotionBlocker::InvalidWalRange)
    );
}

#[test]
fn malformed_wal_range_count_mismatch_fails_closed_with_invalid_range_blocker() {
    let decision = evidence(
        Some(10),
        Some(WalShipmentRange {
            first: Lsn::new(11),
            last: Lsn::new(20),
            count: 9,
        }),
        1,
        20,
        20,
        ResyncCompatibility::Compatible,
    )
    .decide();

    assert_eq!(decision.lag_status, ResyncLagStatus::WalGap);
    assert_eq!(
        decision.promotion_eligibility,
        ReplicaPromotionEligibility::Blocked(ReplicaPromotionBlocker::InvalidWalRange)
    );
}
