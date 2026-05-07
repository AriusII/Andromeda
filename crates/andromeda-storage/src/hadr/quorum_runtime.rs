//! F3 Quorum Runtime Sequencing — Core Membership and Consensus Logic
//!
//! This module owns the runtime orchestration of quorum consensus for WAL shipping
//! decisions and replica promotion eligibility in Andromeda's single-primary topology.
//!
//! # Design Principles
//!
//! - **Pure Functions:** All decision logic is deterministic and replay-safe (no I/O, no time).
//! - **Membership Epoch:** Every topology change (join, suspect, promote) increments the epoch.
//! - **Write Admission Control:** Requires configurable min quorum ACK before commit visibility.
//! - **Promotion Eligibility:** Rank replicas by LSN distance from primary (tie-break by replica ID).
//! - **Fencing Decision:** Pure function mapping (membership state, LSN state, policy) -> Block/Allow.
//! - **No Byzantine Assumptions:** V0 assumes benign failures only; no Byzantine-resistant logic.
//!
//! # Module Exports
//!
//! - [`ReplicaMember`] — Replica identity + health state (Alive, Suspect, Dead).
//! - [`QuorumMembership`] — Set of active replicas + membership epoch.
//! - [`QuorumConsensus`] — Write admission decision (require min ACKs).
//! - [`PromotionRank`] — Replica rank by LSN distance.
//! - [`FencingDecision`] — Allow/Block policy.
//! - [`FencingPolicy`] — Async vs QuorumEnforced modes.
//! - [`FencingEvent`] — Categorizes fencing triggers (disconnect, checksum, gap).

mod consensus;
mod fencing;
mod membership;
mod promotion;

pub use consensus::*;
pub use fencing::*;
pub use membership::*;
pub use promotion::*;

const fn majority_quorum_size(member_count: usize) -> usize {
    member_count / 2 + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Lsn;

    #[test]
    fn replica_member_lsn_distance() {
        let replica = ReplicaMember::new(1, Lsn::new(100), Lsn::new(100));
        let primary_durable = Lsn::new(500);

        let distance = replica.lsn_distance_from(primary_durable);
        assert_eq!(distance, 400);
    }

    #[test]
    fn replica_promotion_eligible_when_caught_up() {
        let replica = ReplicaMember::new(1, Lsn::new(500), Lsn::new(500));
        let primary_durable = Lsn::new(500);

        assert!(replica.is_promotion_eligible(primary_durable));
    }

    #[test]
    fn replica_not_eligible_when_lagging() {
        let replica = ReplicaMember::new(1, Lsn::new(400), Lsn::new(400));
        let primary_durable = Lsn::new(500);

        assert!(!replica.is_promotion_eligible(primary_durable));
    }

    #[test]
    fn membership_creation_rejects_empty() {
        let result = QuorumMembership::new(1, vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn membership_quorum_size_majority() -> Result<(), QuorumMembershipRejection> {
        let replicas = vec![
            ReplicaMember::new(2, Lsn::new(0), Lsn::new(0)),
            ReplicaMember::new(3, Lsn::new(0), Lsn::new(0)),
            ReplicaMember::new(4, Lsn::new(0), Lsn::new(0)),
        ];
        let membership = QuorumMembership::new(1, replicas)?;

        // 3 replicas => quorum_size = 3/2 + 1 = 2
        assert_eq!(membership.quorum_size(), 2);
        assert_eq!(membership.size(), 3);
        Ok(())
    }

    #[test]
    fn membership_quorum_size_uses_strict_majority_for_even_membership()
    -> Result<(), QuorumMembershipRejection> {
        let replicas = vec![
            ReplicaMember::new(2, Lsn::new(0), Lsn::new(0)),
            ReplicaMember::new(3, Lsn::new(0), Lsn::new(0)),
        ];
        let membership = QuorumMembership::new(1, replicas)?;

        assert_eq!(membership.quorum_size(), 2);
        assert!(membership.has_quorum());
        Ok(())
    }

    #[test]
    fn fencing_decision_async_always_allows() -> Result<(), QuorumMembershipRejection> {
        let replicas = vec![ReplicaMember::new(2, Lsn::new(0), Lsn::new(0))];
        let membership = QuorumMembership::new(1, replicas)?;

        let decision = decide_fencing(
            &membership,
            FencingEvent::ReplicaDisconnected,
            FencingPolicy::BlockOnQuorumLoss,
            ReplicationMode::Asynchronous,
        );

        assert_eq!(decision, FencingDecision::Allow);
        Ok(())
    }

    #[test]
    fn fencing_decision_quorum_blocks_on_lost_quorum() -> Result<(), QuorumMembershipRejection> {
        let replicas = vec![
            ReplicaMember::new(2, Lsn::new(0), Lsn::new(0)),
            ReplicaMember::new(3, Lsn::new(0), Lsn::new(0)),
        ];
        let mut membership = QuorumMembership::new(1, replicas)?;

        // Mark both replicas dead => lost quorum (need 2 alive, have 0).
        membership.mark_dead(2)?;
        membership.mark_dead(3)?;

        let decision = decide_fencing(
            &membership,
            FencingEvent::ReplicaDisconnected,
            FencingPolicy::BlockOnQuorumLoss,
            ReplicationMode::QuorumEnforced,
        );

        assert_eq!(decision, FencingDecision::Block);
        Ok(())
    }

    #[test]
    fn consensus_async_admits_without_acks() {
        let consensus = QuorumConsensus::async_mode();
        assert!(consensus.can_admit_write(0));
    }

    #[test]
    fn consensus_quorum_requires_min_acks() {
        let consensus = QuorumConsensus::new(2);
        assert!(!consensus.can_admit_write(1));
        assert!(consensus.can_admit_write(2));
    }

    #[test]
    fn consensus_from_membership_majority_uses_strict_majority() {
        assert_eq!(
            QuorumConsensus::from_membership_majority(2).min_quorum_acks,
            2
        );
        assert_eq!(
            QuorumConsensus::from_membership_majority(3).min_quorum_acks,
            2
        );
    }

    #[test]
    fn promotion_best_candidate_by_lsn_distance() -> Result<(), QuorumMembershipRejection> {
        let ranks = vec![
            PromotionRank {
                replica_id: 1,
                lsn_distance: 100,
                is_eligible: true,
            },
            PromotionRank {
                replica_id: 2,
                lsn_distance: 50,
                is_eligible: true,
            },
        ];

        let best = PromotionRank::best_candidate(&ranks)
            .ok_or(QuorumMembershipRejection::EmptyReplicaSet)?;
        assert_eq!(best.replica_id, 2);
        Ok(())
    }

    #[test]
    fn promotion_best_candidate_tie_break_by_id() -> Result<(), QuorumMembershipRejection> {
        let ranks = vec![
            PromotionRank {
                replica_id: 5,
                lsn_distance: 100,
                is_eligible: true,
            },
            PromotionRank {
                replica_id: 2,
                lsn_distance: 100,
                is_eligible: true,
            },
        ];

        let best = PromotionRank::best_candidate(&ranks)
            .ok_or(QuorumMembershipRejection::EmptyReplicaSet)?;
        assert_eq!(best.replica_id, 2);
        Ok(())
    }

    #[test]
    fn promotion_best_candidate_returns_none_without_eligible_replica() {
        let ranks = vec![
            PromotionRank {
                replica_id: 1,
                lsn_distance: 0,
                is_eligible: false,
            },
            PromotionRank {
                replica_id: 2,
                lsn_distance: 10,
                is_eligible: false,
            },
        ];

        assert_eq!(PromotionRank::best_candidate(&ranks), None);
    }

    #[test]
    fn membership_epoch_increments_on_topology_change() -> Result<(), QuorumMembershipRejection> {
        let replicas = vec![
            ReplicaMember::new(2, Lsn::new(0), Lsn::new(0)),
            ReplicaMember::new(3, Lsn::new(0), Lsn::new(0)),
        ];
        let mut membership = QuorumMembership::new(1, replicas)?;

        let initial_epoch = membership.epoch();
        membership.mark_suspect(2)?;
        assert_eq!(membership.epoch(), initial_epoch + 1);

        membership.mark_dead(3)?;
        assert_eq!(membership.epoch(), initial_epoch + 2);
        Ok(())
    }
}
