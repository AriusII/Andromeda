use andromeda_wal::Lsn;

use super::WalShipmentRange;

/// Local compatibility evidence for a replica catch-up decision.
///
/// This is an in-memory decision input only. It is not a persisted or wire
/// format and must be rebuilt from durable snapshot/WAL/manifest evidence by
/// the runtime owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResyncCompatibility {
    Compatible,
    Incompatible,
}

/// Local lag classification for a replica catch-up/resync decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResyncLagStatus {
    MissingSnapshot,
    RetentionFloorViolation,
    WalGap,
    Lagging { lag_lsn: u64 },
    CaughtUp,
}

/// First fail-closed reason that blocks promotion from local catch-up evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicaPromotionBlocker {
    MissingSnapshot,
    RetentionFloorViolation,
    WalGap,
    InvalidWalRange,
    LsnOverflow,
    ReplicaLagging,
    IncompatibleReplica,
}

/// Local promotion eligibility derived from snapshot, WAL, retention and safe
/// LSN evidence. This does not replace quorum/fencing/promotion modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicaPromotionEligibility {
    Eligible,
    Blocked(ReplicaPromotionBlocker),
}

/// Local evidence used to decide whether a replica can catch up from a
/// snapshot base plus a contiguous WAL range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplicaResyncEvidence {
    pub snapshot_base_lsn: Option<Lsn>,
    pub wal_range: Option<WalShipmentRange>,
    pub retention_floor_lsn: Lsn,
    pub replica_safe_lsn: Lsn,
    pub primary_tip_lsn: Lsn,
    pub max_promotion_lag_lsn: u64,
    pub compatibility: ResyncCompatibility,
}

impl ReplicaResyncEvidence {
    pub const fn new(
        snapshot_base_lsn: Option<Lsn>,
        wal_range: Option<WalShipmentRange>,
        retention_floor_lsn: Lsn,
        replica_safe_lsn: Lsn,
        primary_tip_lsn: Lsn,
        max_promotion_lag_lsn: u64,
        compatibility: ResyncCompatibility,
    ) -> Self {
        Self {
            snapshot_base_lsn,
            wal_range,
            retention_floor_lsn,
            replica_safe_lsn,
            primary_tip_lsn,
            max_promotion_lag_lsn,
            compatibility,
        }
    }

    pub fn decide(self) -> ReplicaResyncDecision {
        ReplicaResyncDecision::from_evidence(self)
    }
}

/// Deterministic local catch-up decision. It is advisory boundary evidence for
/// callers and must not be treated as durable system truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplicaResyncDecision {
    pub snapshot_base_lsn: Option<Lsn>,
    pub required_wal_range: Option<WalShipmentRange>,
    pub observed_wal_range: Option<WalShipmentRange>,
    pub retention_floor_lsn: Lsn,
    pub lag_status: ResyncLagStatus,
    pub promotion_eligibility: ReplicaPromotionEligibility,
}

impl ReplicaResyncDecision {
    pub fn from_evidence(evidence: ReplicaResyncEvidence) -> Self {
        let Some(snapshot_base_lsn) = evidence.snapshot_base_lsn else {
            return blocked_decision(evidence, None, ResyncLagStatus::MissingSnapshot);
        };

        if let Some(observed_range) = evidence.wal_range
            && validate_wal_range(observed_range).is_err()
        {
            return Self {
                snapshot_base_lsn: Some(snapshot_base_lsn),
                required_wal_range: None,
                observed_wal_range: evidence.wal_range,
                retention_floor_lsn: evidence.retention_floor_lsn,
                lag_status: ResyncLagStatus::WalGap,
                promotion_eligibility: ReplicaPromotionEligibility::Blocked(
                    ReplicaPromotionBlocker::InvalidWalRange,
                ),
            };
        }

        let required_wal_range = if evidence.primary_tip_lsn > snapshot_base_lsn {
            match required_range(snapshot_base_lsn, evidence.primary_tip_lsn) {
                Some(range) => Some(range),
                None => {
                    return blocked_decision(evidence, None, ResyncLagStatus::WalGap)
                        .with_blocker(ReplicaPromotionBlocker::LsnOverflow);
                },
            }
        } else {
            None
        };

        if let Some(required_range) = required_wal_range {
            if required_range.first < evidence.retention_floor_lsn {
                return blocked_decision(
                    evidence,
                    Some(required_range),
                    ResyncLagStatus::RetentionFloorViolation,
                );
            }
            if !observed_range_covers_required(evidence.wal_range, required_range) {
                return blocked_decision(evidence, Some(required_range), ResyncLagStatus::WalGap);
            }
        }

        let lag_status = lag_status(evidence.replica_safe_lsn, evidence.primary_tip_lsn);
        let promotion_eligibility = promotion_eligibility(evidence, lag_status);
        Self {
            snapshot_base_lsn: evidence.snapshot_base_lsn,
            required_wal_range,
            observed_wal_range: evidence.wal_range,
            retention_floor_lsn: evidence.retention_floor_lsn,
            lag_status,
            promotion_eligibility,
        }
    }

    fn with_blocker(mut self, blocker: ReplicaPromotionBlocker) -> Self {
        self.promotion_eligibility = ReplicaPromotionEligibility::Blocked(blocker);
        self
    }
}

fn blocked_decision(
    evidence: ReplicaResyncEvidence,
    required_wal_range: Option<WalShipmentRange>,
    lag_status: ResyncLagStatus,
) -> ReplicaResyncDecision {
    let blocker = match lag_status {
        ResyncLagStatus::MissingSnapshot => ReplicaPromotionBlocker::MissingSnapshot,
        ResyncLagStatus::RetentionFloorViolation => {
            ReplicaPromotionBlocker::RetentionFloorViolation
        },
        ResyncLagStatus::WalGap => ReplicaPromotionBlocker::WalGap,
        ResyncLagStatus::Lagging { .. } => ReplicaPromotionBlocker::ReplicaLagging,
        ResyncLagStatus::CaughtUp => ReplicaPromotionBlocker::IncompatibleReplica,
    };
    ReplicaResyncDecision {
        snapshot_base_lsn: evidence.snapshot_base_lsn,
        required_wal_range,
        observed_wal_range: evidence.wal_range,
        retention_floor_lsn: evidence.retention_floor_lsn,
        lag_status,
        promotion_eligibility: ReplicaPromotionEligibility::Blocked(blocker),
    }
}

fn required_range(snapshot_base_lsn: Lsn, primary_tip_lsn: Lsn) -> Option<WalShipmentRange> {
    let first = snapshot_base_lsn.checked_next()?;
    let span = primary_tip_lsn
        .get()
        .checked_sub(first.get())?
        .checked_add(1)?;
    let count = usize::try_from(span).ok()?;
    Some(WalShipmentRange {
        first,
        last: primary_tip_lsn,
        count,
    })
}

fn observed_range_covers_required(
    observed: Option<WalShipmentRange>,
    required: WalShipmentRange,
) -> bool {
    observed.is_some_and(|range| range.first <= required.first && range.last >= required.last)
}

fn validate_wal_range(range: WalShipmentRange) -> Result<(), ()> {
    if range.count == 0 || range.first > range.last {
        return Err(());
    }
    let span = range
        .last
        .get()
        .checked_sub(range.first.get())
        .and_then(|delta| delta.checked_add(1))
        .ok_or(())?;
    let count = u64::try_from(range.count).map_err(|_| ())?;
    if span == count { Ok(()) } else { Err(()) }
}

fn lag_status(replica_safe_lsn: Lsn, primary_tip_lsn: Lsn) -> ResyncLagStatus {
    if replica_safe_lsn >= primary_tip_lsn {
        ResyncLagStatus::CaughtUp
    } else {
        ResyncLagStatus::Lagging {
            lag_lsn: primary_tip_lsn.get() - replica_safe_lsn.get(),
        }
    }
}

fn promotion_eligibility(
    evidence: ReplicaResyncEvidence,
    lag_status: ResyncLagStatus,
) -> ReplicaPromotionEligibility {
    if evidence.compatibility != ResyncCompatibility::Compatible {
        return ReplicaPromotionEligibility::Blocked(ReplicaPromotionBlocker::IncompatibleReplica);
    }
    match lag_status {
        ResyncLagStatus::CaughtUp => ReplicaPromotionEligibility::Eligible,
        ResyncLagStatus::Lagging { lag_lsn } if lag_lsn <= evidence.max_promotion_lag_lsn => {
            ReplicaPromotionEligibility::Eligible
        },
        ResyncLagStatus::Lagging { .. } => {
            ReplicaPromotionEligibility::Blocked(ReplicaPromotionBlocker::ReplicaLagging)
        },
        ResyncLagStatus::MissingSnapshot => {
            ReplicaPromotionEligibility::Blocked(ReplicaPromotionBlocker::MissingSnapshot)
        },
        ResyncLagStatus::RetentionFloorViolation => {
            ReplicaPromotionEligibility::Blocked(ReplicaPromotionBlocker::RetentionFloorViolation)
        },
        ResyncLagStatus::WalGap => {
            ReplicaPromotionEligibility::Blocked(ReplicaPromotionBlocker::WalGap)
        },
    }
}
