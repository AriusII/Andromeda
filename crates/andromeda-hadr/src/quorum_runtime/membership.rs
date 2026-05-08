use super::majority_quorum_size;
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

/// Typed rejection reason for malformed quorum membership snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuorumMembershipRejection {
    EmptyReplicaSet,
    DuplicateReplicaId,
    ReplicaMatchesPrimary,
    EpochOverflow,
}

impl QuorumMembershipRejection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmptyReplicaSet => "HADR quorum membership must contain at least one replica",
            Self::DuplicateReplicaId => "HADR quorum membership contains duplicate replica ids",
            Self::ReplicaMatchesPrimary => {
                "HADR quorum membership replica id must not match primary id"
            },
            Self::EpochOverflow => "HADR quorum membership epoch overflow",
        }
    }
}

impl std::fmt::Display for QuorumMembershipRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::error::Error for QuorumMembershipRejection {}

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
    /// Map of replica_id -> ReplicaMember.
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
    pub fn new(
        primary_id: u64,
        replicas: Vec<ReplicaMember>,
    ) -> Result<Self, QuorumMembershipRejection> {
        if replicas.is_empty() {
            return Err(QuorumMembershipRejection::EmptyReplicaSet);
        }

        let mut members = HashMap::new();
        for replica in replicas {
            if members.contains_key(&replica.replica_id) {
                return Err(QuorumMembershipRejection::DuplicateReplicaId);
            }
            if replica.replica_id == primary_id {
                return Err(QuorumMembershipRejection::ReplicaMatchesPrimary);
            }
            members.insert(replica.replica_id, replica);
        }

        let quorum_size = majority_quorum_size(members.len());

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
    pub fn increment_epoch(&mut self) -> Result<(), QuorumMembershipRejection> {
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or(QuorumMembershipRejection::EpochOverflow)?;
        Ok(())
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
    pub fn mark_suspect(&mut self, replica_id: u64) -> Result<(), QuorumMembershipRejection> {
        if let Some(replica) = self.members.get_mut(&replica_id)
            && replica.health_state.is_alive()
        {
            replica.health_state = ReplicaHealthState::Suspect;
            self.increment_epoch()?;
        }
        Ok(())
    }

    /// Mark a replica as dead (connection permanently lost).
    pub fn mark_dead(&mut self, replica_id: u64) -> Result<(), QuorumMembershipRejection> {
        if let Some(replica) = self.members.get_mut(&replica_id)
            && !replica.health_state.is_dead()
        {
            replica.health_state = ReplicaHealthState::Dead;
            self.increment_epoch()?;
        }
        Ok(())
    }

    /// Mark a suspect replica as alive again (reconnected).
    pub fn mark_alive(&mut self, replica_id: u64) -> Result<(), QuorumMembershipRejection> {
        if let Some(replica) = self.members.get_mut(&replica_id)
            && replica.health_state.is_suspect()
        {
            replica.health_state = ReplicaHealthState::Alive;
            self.increment_epoch()?;
        }
        Ok(())
    }

    /// Update a replica's LSN state (received_lsn, shipped_lsn).
    pub fn update_replica_lsn(&mut self, replica_id: u64, received_lsn: Lsn, shipped_lsn: Lsn) {
        if let Some(replica) = self.members.get_mut(&replica_id) {
            replica.received_lsn = received_lsn;
            replica.shipped_lsn = shipped_lsn;
        }
    }

    /// Remove a dead replica from membership entirely.
    pub fn remove_dead(&mut self, replica_id: u64) -> Result<(), QuorumMembershipRejection> {
        if self
            .members
            .get(&replica_id)
            .is_some_and(|replica| replica.health_state.is_dead())
        {
            self.members.remove(&replica_id);
            // Recompute quorum size if we had replicas.
            if !self.members.is_empty() {
                self.quorum_size = majority_quorum_size(self.members.len());
            }
            self.increment_epoch()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn membership_epoch_overflow_is_rejected() -> Result<(), QuorumMembershipRejection> {
        let replicas = vec![ReplicaMember::new(2, Lsn::new(0), Lsn::new(0))];
        let mut membership = QuorumMembership::new(1, replicas)?;
        membership.epoch = u64::MAX;

        let err = membership
            .mark_suspect(2)
            .expect_err("topology change must fail at epoch overflow");

        assert_eq!(err, QuorumMembershipRejection::EpochOverflow);
        Ok(())
    }
}
