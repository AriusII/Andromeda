use super::membership::{QuorumMembership, ReplicaMember};
use crate::Lsn;

/// Promotion eligibility ranking for a replica.
///
/// Replicas are ranked by LSN distance from primary (shortest distance = best).
/// On tie, rank by replica ID (lower ID = better).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromotionRank {
    /// Replica's ID.
    pub replica_id: u64,
    /// Distance in LSN bytes from primary's durable_lsn.
    /// Negative = replica ahead (shouldn't happen).
    /// Zero = replica fully caught up.
    /// Positive = replica lagging by this many bytes.
    pub lsn_distance: i64,
    /// True if this replica is promotable (alive and caught up).
    pub is_eligible: bool,
}

impl PromotionRank {
    /// Build eligibility rank for a replica.
    pub fn new(replica: &ReplicaMember, primary_durable_lsn: Lsn) -> Self {
        let lsn_distance = replica.lsn_distance_from(primary_durable_lsn);
        let is_eligible = replica.is_promotion_eligible(primary_durable_lsn);

        Self {
            replica_id: replica.replica_id,
            lsn_distance,
            is_eligible,
        }
    }

    /// Sort rankings: best candidate first (lowest distance, lowest ID on tie).
    pub fn best_candidate(ranks: &[PromotionRank]) -> Option<PromotionRank> {
        ranks
            .iter()
            .copied()
            .filter(|rank| rank.is_eligible)
            .min_by(|a, b| {
                a.lsn_distance
                    .cmp(&b.lsn_distance)
                    .then_with(|| a.replica_id.cmp(&b.replica_id))
            })
    }
}

/// Compute promotion eligibility for all replicas in a membership.
pub fn rank_promotion_candidates(
    membership: &QuorumMembership,
    primary_durable_lsn: Lsn,
) -> Vec<PromotionRank> {
    membership
        .members_sorted()
        .iter()
        .map(|replica| PromotionRank::new(replica, primary_durable_lsn))
        .collect()
}

/// Get the best promotion candidate (if any).
pub fn select_promotion_candidate(
    membership: &QuorumMembership,
    primary_durable_lsn: Lsn,
) -> Option<PromotionRank> {
    let ranks = rank_promotion_candidates(membership, primary_durable_lsn);
    PromotionRank::best_candidate(&ranks)
}
