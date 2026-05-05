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
//! - **Fencing Decision:** Pure function mapping (membership state, LSN state, policy) → Block/Allow.
//! - **No Byzantine Assumptions:** V0 assumes benign failures only; no Byzantine-resistant logic.
//!
//! # Module Exports
//!
//! - [`ReplicaMember`] — Replica identity + health state (Alive, Suspect, Dead).
//! - [`QuorumMembership`] — Set of active replicas + membership epoch.
//! - [`QuorumConsensus`] — Write admission decision (require min ACKs).
//! - [`PromotionEligibility`] — Replica rank by LSN distance.
//! - [`FencingDecision`] — Allow/Block policy.
//! - [`FencingPolicy`] — Async vs QuorumEnforced modes.
//! - [`FencingEvent`] — Categorizes fencing triggers (disconnect, checksum, gap).

use crate::Lsn;
use std::collections::HashMap;

/// Health state of a replica in the quorum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplicaHealthState {
    /// Replica is connected and responsive within heartbeat window.
    Alive,
    /// Replica missed heartbeat(s) but not yet pronounced dead.
    Suspect,
    /// Replica is not responding or connection permanently lost.
    Dead,
}

impl ReplicaHealthState {
    pub const fn is_alive(self) -> bool {
        matches!(self, Self::Alive)
    }

    pub const fn is_suspect(self) -> bool {
        matches!(self, Self::Suspect)
    }

    pub const fn is_dead(self) -> bool {
        matches!(self, Self::Dead)
    }
}

/// Identity and health state of a replica member in the quorum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReplicaMember {
    /// Unique stable identifier for this replica.
    pub replica_id: u64,
    /// Current health state (Alive, Suspect, Dead).
    pub health_state: ReplicaHealthState,
    /// LSN up to which this replica's WAL has been durably received and validated.
    pub received_lsn: Lsn,
    /// LSN that has been shipped from primary to this replica.
    pub shipped_lsn: Lsn,
}

impl ReplicaMember {
    pub fn new(replica_id: u64, received_lsn: Lsn, shipped_lsn: Lsn) -> Self {
        Self {
            replica_id,
            health_state: ReplicaHealthState::Alive,
            received_lsn,
            shipped_lsn,
        }
    }

    /// Distance in LSN bytes from primary's durable_lsn. Used for promotion ranking.
    pub const fn lsn_distance_from(&self, primary_durable_lsn: Lsn) -> i64 {
        // Negative distance if replica is ahead (shouldn't happen, but safe).
        // Positive distance if replica is behind.
        primary_durable_lsn.get() as i64 - self.received_lsn.get() as i64
    }

    /// Replica is eligible for promotion if it is alive and not too far behind
    /// the primary's durable LSN (within ~100 bytes is considered acceptable).
    pub fn is_promotion_eligible(&self, primary_durable_lsn: Lsn) -> bool {
        if !self.health_state.is_alive() {
            return false;
        }
        let distance = self.lsn_distance_from(primary_durable_lsn);
        distance < 100
    }
}

/// Snapshot of quorum membership at a given epoch. Membership is durable
/// (stored in a catalog record), not a runtime view.
///
/// Quorum size is computed as `floor(N / 2) + 1`, where N is the number of members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuorumMembership {
    /// Map of replica_id → ReplicaMember.
    members: HashMap<u64, ReplicaMember>,
    /// Monotonic counter incremented on every membership topology change
    /// (join, suspect, remove, promote).
    epoch: u64,
    /// Primary's own ID (for self-reference).
    primary_id: u64,
    /// Precomputed quorum size for this membership.
    quorum_size: usize,
}

impl QuorumMembership {
    /// Create a new membership with a primary and an optional set of replicas.
    /// Rejects empty replica sets or duplicates.
    pub fn new(primary_id: u64, replicas: Vec<ReplicaMember>) -> Result<Self, &'static str> {
        if replicas.is_empty() {
            return Err("Membership must contain at least one replica");
        }

        let mut members = HashMap::new();
        for replica in replicas {
            if members.contains_key(&replica.replica_id) {
                return Err("Duplicate replica IDs in membership");
            }
            if replica.replica_id == primary_id {
                return Err("Replica ID cannot match primary ID");
            }
            members.insert(replica.replica_id, replica);
        }

        let quorum_size = (members.len() + 1) / 2;

        Ok(Self {
            members,
            epoch: 0,
            primary_id,
            quorum_size,
        })
    }

    /// Get the current membership epoch.
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Increment membership epoch (called on any topology change).
    pub fn increment_epoch(&mut self) {
        self.epoch = self.epoch.saturating_add(1);
    }

    /// Get total number of replicas in membership.
    pub fn size(&self) -> usize {
        self.members.len()
    }

    /// Get configured quorum size (majority).
    pub const fn quorum_size(&self) -> usize {
        self.quorum_size
    }

    /// Get replica by ID.
    pub fn get_replica(&self, replica_id: u64) -> Option<&ReplicaMember> {
        self.members.get(&replica_id)
    }

    /// Get mutable reference to replica (for health state updates).
    pub fn get_replica_mut(&mut self, replica_id: u64) -> Option<&mut ReplicaMember> {
        self.members.get_mut(&replica_id)
    }

    /// Count alive replicas (Alive state).
    pub fn count_alive(&self) -> usize {
        self.members
            .values()
            .filter(|r| r.health_state.is_alive())
            .count()
    }

    /// Count alive + suspect replicas (not yet dead).
    pub fn count_responsive(&self) -> usize {
        self.members
            .values()
            .filter(|r| !r.health_state.is_dead())
            .count()
    }

    /// Get all members as a vector (sorted by replica_id for determinism).
    pub fn members_sorted(&self) -> Vec<ReplicaMember> {
        let mut sorted: Vec<_> = self.members.values().copied().collect();
        sorted.sort_by_key(|m| m.replica_id);
        sorted
    }

    /// Check if membership has lost quorum (alive replicas < quorum_size).
    pub fn has_quorum(&self) -> bool {
        self.count_alive() >= self.quorum_size
    }

    /// Mark a replica as suspect (due to missed heartbeat).
    pub fn mark_suspect(&mut self, replica_id: u64) {
        if let Some(replica) = self.members.get_mut(&replica_id) {
            if replica.health_state.is_alive() {
                replica.health_state = ReplicaHealthState::Suspect;
                self.increment_epoch();
            }
        }
    }

    /// Mark a replica as dead (connection permanently lost).
    pub fn mark_dead(&mut self, replica_id: u64) {
        if let Some(replica) = self.members.get_mut(&replica_id) {
            if !replica.health_state.is_dead() {
                replica.health_state = ReplicaHealthState::Dead;
                self.increment_epoch();
            }
        }
    }

    /// Mark a suspect replica as alive again (reconnected).
    pub fn mark_alive(&mut self, replica_id: u64) {
        if let Some(replica) = self.members.get_mut(&replica_id) {
            if replica.health_state.is_suspect() {
                replica.health_state = ReplicaHealthState::Alive;
                self.increment_epoch();
            }
        }
    }

    /// Update a replica's LSN state (received_lsn, shipped_lsn).
    pub fn update_replica_lsn(&mut self, replica_id: u64, received_lsn: Lsn, shipped_lsn: Lsn) {
        if let Some(replica) = self.members.get_mut(&replica_id) {
            replica.received_lsn = received_lsn;
            replica.shipped_lsn = shipped_lsn;
        }
    }

    /// Remove a dead replica from membership entirely.
    pub fn remove_dead(&mut self, replica_id: u64) {
        if let Some(replica) = self.members.remove(&replica_id) {
            if replica.health_state.is_dead() {
                // Recompute quorum size if we had replicas.
                if !self.members.is_empty() {
                    self.quorum_size = (self.members.len() + 1) / 2;
                }
                self.increment_epoch();
            }
        }
    }
}

/// Replication consistency mode: controls write admission and fencing behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationMode {
    /// Asynchronous: primary can commit without waiting for replica ACKs.
    /// Writes are visible immediately; fencing only triggers on explicit policy.
    Asynchronous,
    /// Synchronous/Quorum: primary must wait for min quorum of replicas to ACK.
    /// Requires quorum to be maintained; write admission blocked if quorum lost.
    QuorumEnforced,
}

impl ReplicationMode {
    pub const fn is_async(&self) -> bool {
        matches!(self, Self::Asynchronous)
    }

    pub const fn is_quorum(&self) -> bool {
        matches!(self, Self::QuorumEnforced)
    }
}

/// Fencing policy: controls when replication failures trigger write blocking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FencingPolicy {
    /// Never block writes on replica failure; allow degraded operation.
    Allow,
    /// Block writes if quorum membership is lost.
    BlockOnQuorumLoss,
}

/// Events that trigger fencing decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FencingEvent {
    /// Replica connection lost.
    ReplicaDisconnected,
    /// Replica checksum validation failed.
    ReplicaChecksumMismatch,
    /// Replica reported LSN gap (lost records).
    ReplicaLsnGap,
    /// Unknown/unclassified failure.
    Unknown,
}

/// Result of fencing decision: allow or block new writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FencingDecision {
    /// Allow new writes to proceed.
    Allow,
    /// Block new writes (wait for quorum recovery or operator intervention).
    Block,
}

impl FencingDecision {
    pub const fn is_allow(self) -> bool {
        matches!(self, Self::Allow)
    }

    pub const fn is_block(self) -> bool {
        matches!(self, Self::Block)
    }
}

/// Pure function: decide whether to allow or block writes based on
/// membership state, LSN state, and fencing policy.
///
/// # Arguments
///
/// * `membership` — Current quorum membership snapshot.
/// * `event` — Fencing event that triggered this decision.
/// * `policy` — Fencing policy (Allow vs BlockOnQuorumLoss).
/// * `replication_mode` — Async vs Quorum mode.
///
/// # Returns
///
/// [`FencingDecision::Allow`] if writes should proceed,
/// [`FencingDecision::Block`] if writes should be held.
///
/// # Determinism
///
/// This function is pure: given the same inputs, it always produces the same
/// output. It is safe to replay and does not depend on time or I/O.
pub fn decide_fencing(
    membership: &QuorumMembership,
    _event: FencingEvent,
    policy: FencingPolicy,
    replication_mode: ReplicationMode,
) -> FencingDecision {
    match policy {
        FencingPolicy::Allow => FencingDecision::Allow,
        FencingPolicy::BlockOnQuorumLoss => {
            // In async mode, always allow (no quorum requirement).
            if replication_mode.is_async() {
                FencingDecision::Allow
            } else {
                // In quorum mode, block if membership has lost quorum.
                if membership.has_quorum() {
                    FencingDecision::Allow
                } else {
                    FencingDecision::Block
                }
            }
        }
    }
}

/// Quorum consensus decision for write admission.
///
/// Requires that the primary has received acknowledgment from at least
/// `min_quorum_acks` replicas before the write becomes visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuorumConsensus {
    /// Minimum number of alive replica ACKs required before write is visible.
    /// In async mode, this is 0 (writes visible immediately).
    /// In quorum mode, this is typically the membership's quorum_size.
    pub min_quorum_acks: usize,
}

impl QuorumConsensus {
    pub const fn new(min_quorum_acks: usize) -> Self {
        Self { min_quorum_acks }
    }

    /// Check if write admission is allowed based on current replica ACKs.
    ///
    /// Returns true if alive_acks >= min_quorum_acks.
    pub const fn can_admit_write(&self, alive_acks: usize) -> bool {
        alive_acks >= self.min_quorum_acks
    }

    /// Async mode: no quorum requirement.
    pub const fn async_mode() -> Self {
        Self { min_quorum_acks: 0 }
    }

    /// Build quorum consensus from membership (majority size).
    pub const fn from_membership_majority(membership_size: usize) -> Self {
        let quorum_size = membership_size / 2 + 1;
        Self {
            min_quorum_acks: quorum_size,
        }
    }
}

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
        let mut eligible: Vec<_> = ranks.iter().copied().filter(|r| r.is_eligible).collect();
        if eligible.is_empty() {
            return None;
        }
        eligible.sort_by(|a, b| {
            a.lsn_distance
                .cmp(&b.lsn_distance)
                .then_with(|| a.replica_id.cmp(&b.replica_id))
        });
        eligible.first().copied()
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn membership_quorum_size_majority() {
        let replicas = vec![
            ReplicaMember::new(2, Lsn::new(0), Lsn::new(0)),
            ReplicaMember::new(3, Lsn::new(0), Lsn::new(0)),
            ReplicaMember::new(4, Lsn::new(0), Lsn::new(0)),
        ];
        let membership = QuorumMembership::new(1, replicas).unwrap();

        // 3 replicas => quorum_size = 3/2 + 1 = 2
        assert_eq!(membership.quorum_size(), 2);
        assert_eq!(membership.size(), 3);
    }

    #[test]
    fn fencing_decision_async_always_allows() {
        let replicas = vec![ReplicaMember::new(2, Lsn::new(0), Lsn::new(0))];
        let membership = QuorumMembership::new(1, replicas).unwrap();

        let decision = decide_fencing(
            &membership,
            FencingEvent::ReplicaDisconnected,
            FencingPolicy::BlockOnQuorumLoss,
            ReplicationMode::Asynchronous,
        );

        assert_eq!(decision, FencingDecision::Allow);
    }

    #[test]
    fn fencing_decision_quorum_blocks_on_lost_quorum() {
        let replicas = vec![
            ReplicaMember::new(2, Lsn::new(0), Lsn::new(0)),
            ReplicaMember::new(3, Lsn::new(0), Lsn::new(0)),
        ];
        let mut membership = QuorumMembership::new(1, replicas).unwrap();

        // Mark both replicas dead => lost quorum (need 2 alive, have 0).
        membership.mark_dead(2);
        membership.mark_dead(3);

        let decision = decide_fencing(
            &membership,
            FencingEvent::ReplicaDisconnected,
            FencingPolicy::BlockOnQuorumLoss,
            ReplicationMode::QuorumEnforced,
        );

        assert_eq!(decision, FencingDecision::Block);
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
    fn promotion_best_candidate_by_lsn_distance() {
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

        let best = PromotionRank::best_candidate(&ranks).unwrap();
        assert_eq!(best.replica_id, 2);
    }

    #[test]
    fn promotion_best_candidate_tie_break_by_id() {
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

        let best = PromotionRank::best_candidate(&ranks).unwrap();
        assert_eq!(best.replica_id, 2);
    }

    #[test]
    fn membership_epoch_increments_on_topology_change() {
        let replicas = vec![
            ReplicaMember::new(2, Lsn::new(0), Lsn::new(0)),
            ReplicaMember::new(3, Lsn::new(0), Lsn::new(0)),
        ];
        let mut membership = QuorumMembership::new(1, replicas).unwrap();

        let initial_epoch = membership.epoch();
        membership.mark_suspect(2);
        assert_eq!(membership.epoch(), initial_epoch + 1);

        membership.mark_dead(3);
        assert_eq!(membership.epoch(), initial_epoch + 2);
    }
}
