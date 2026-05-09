use andromeda_error::AndromedaResult;

use crate::Lsn;

use super::storage_error;
use crate::{
    membership_state_validation,
    types::{HadrEpoch, HadrNodeId, HadrNodeRole},
};

/// Durable role record for one HADR node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HadrMembershipNode {
    pub id: HadrNodeId,
    pub role: HadrNodeRole,
    pub role_epoch: HadrEpoch,
}

impl HadrMembershipNode {
    pub const fn new(id: HadrNodeId, role: HadrNodeRole, role_epoch: HadrEpoch) -> Self {
        Self {
            id,
            role,
            role_epoch,
        }
    }
}

/// Durable membership journal record.
///
/// The snapshot is the current truth, while records provide an append-only
/// audit trail that explains how that truth became visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HadrMembershipRecord {
    NodeRegistered {
        node_id: HadrNodeId,
        role: HadrNodeRole,
        epoch: HadrEpoch,
    },
    NodeDeregistered {
        node_id: HadrNodeId,
        epoch: HadrEpoch,
    },
    NodeFenced {
        node_id: HadrNodeId,
        epoch: HadrEpoch,
    },
    NodeRoleUpdated {
        node_id: HadrNodeId,
        role: HadrNodeRole,
        epoch: HadrEpoch,
    },
    EpochAdvanced {
        previous_epoch: HadrEpoch,
        new_epoch: HadrEpoch,
    },
    PrimaryPromoted {
        node_id: HadrNodeId,
        epoch: HadrEpoch,
        committed_safe_lsn: Lsn,
    },
}

impl HadrMembershipRecord {
    pub const fn epoch(self) -> HadrEpoch {
        match self {
            Self::NodeRegistered { epoch, .. }
            | Self::NodeDeregistered { epoch, .. }
            | Self::NodeFenced { epoch, .. }
            | Self::NodeRoleUpdated { epoch, .. }
            | Self::PrimaryPromoted { epoch, .. } => epoch,
            Self::EpochAdvanced { new_epoch, .. } => new_epoch,
        }
    }
}

/// Durable membership snapshot. Nodes are kept sorted by id so persisted bytes
/// are deterministic independent of caller insertion order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrMembershipSnapshot {
    epoch: HadrEpoch,
    nodes: Vec<HadrMembershipNode>,
    records: Vec<HadrMembershipRecord>,
}

impl HadrMembershipSnapshot {
    pub fn new(epoch: HadrEpoch, mut nodes: Vec<HadrMembershipNode>) -> AndromedaResult<Self> {
        nodes.sort_by_key(|node| node.id);
        let snapshot = Self {
            epoch,
            nodes,
            records: Vec::new(),
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn with_records(
        epoch: HadrEpoch,
        mut nodes: Vec<HadrMembershipNode>,
        records: Vec<HadrMembershipRecord>,
    ) -> AndromedaResult<Self> {
        nodes.sort_by_key(|node| node.id);
        let snapshot = Self {
            epoch,
            nodes,
            records,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub const fn empty() -> Self {
        Self {
            epoch: HadrEpoch::ZERO,
            nodes: Vec::new(),
            records: Vec::new(),
        }
    }

    pub const fn epoch(&self) -> HadrEpoch {
        self.epoch
    }

    pub fn nodes(&self) -> &[HadrMembershipNode] {
        &self.nodes
    }

    pub fn records(&self) -> &[HadrMembershipRecord] {
        &self.records
    }

    pub fn get(&self, id: HadrNodeId) -> Option<&HadrMembershipNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn contains(&self, id: HadrNodeId) -> bool {
        self.get(id).is_some()
    }

    pub fn primary(&self) -> Option<&HadrMembershipNode> {
        self.nodes
            .iter()
            .find(|node| node.role == HadrNodeRole::Primary)
    }

    pub(super) fn register_node(
        mut self,
        id: HadrNodeId,
        role: HadrNodeRole,
    ) -> AndromedaResult<HadrMembershipSnapshot> {
        if self.contains(id) {
            return Err(storage_error("HADR membership node id already exists"));
        }
        let next_epoch = self.next_epoch()?;
        let previous_epoch = self.epoch;
        self.epoch = next_epoch;
        self.nodes
            .push(HadrMembershipNode::new(id, role, next_epoch));
        self.push_epoch_advanced(previous_epoch, next_epoch);
        self.records.push(HadrMembershipRecord::NodeRegistered {
            node_id: id,
            role,
            epoch: next_epoch,
        });
        Self::with_records(self.epoch, self.nodes, self.records)
    }

    pub(super) fn update_node_role(
        mut self,
        id: HadrNodeId,
        role: HadrNodeRole,
    ) -> AndromedaResult<HadrMembershipSnapshot> {
        if role == HadrNodeRole::Primary {
            return Err(storage_error(
                "HADR membership primary publication must use the promotion boundary",
            ));
        }
        let next_epoch = self.next_epoch()?;
        let previous_epoch = self.epoch;
        let Some(node) = self.nodes.iter_mut().find(|node| node.id == id) else {
            return Err(storage_error("HADR membership node id is not registered"));
        };

        node.role = role;
        node.role_epoch = next_epoch;
        self.epoch = next_epoch;
        self.push_epoch_advanced(previous_epoch, next_epoch);
        self.records.push(HadrMembershipRecord::NodeRoleUpdated {
            node_id: id,
            role,
            epoch: next_epoch,
        });
        Self::with_records(self.epoch, self.nodes, self.records)
    }

    pub(super) fn deregister_node(
        mut self,
        id: HadrNodeId,
    ) -> AndromedaResult<HadrMembershipSnapshot> {
        if !self.contains(id) {
            return Err(storage_error("HADR membership node id is not registered"));
        }
        let next_epoch = self.next_epoch()?;
        let previous_epoch = self.epoch;
        self.nodes.retain(|node| node.id != id);
        self.epoch = next_epoch;
        self.push_epoch_advanced(previous_epoch, next_epoch);
        self.records.push(HadrMembershipRecord::NodeDeregistered {
            node_id: id,
            epoch: next_epoch,
        });
        Self::with_records(self.epoch, self.nodes, self.records)
    }

    pub(super) fn fence_node(mut self, id: HadrNodeId) -> AndromedaResult<HadrMembershipSnapshot> {
        let next_epoch = self.next_epoch()?;
        let previous_epoch = self.epoch;
        let Some(node) = self.nodes.iter_mut().find(|node| node.id == id) else {
            return Err(storage_error("HADR membership node id is not registered"));
        };

        if node.role != HadrNodeRole::Replica {
            node.role = HadrNodeRole::Replica;
            node.role_epoch = next_epoch;
        }
        self.epoch = next_epoch;
        self.push_epoch_advanced(previous_epoch, next_epoch);
        self.records.push(HadrMembershipRecord::NodeFenced {
            node_id: id,
            epoch: next_epoch,
        });
        Self::with_records(self.epoch, self.nodes, self.records)
    }

    pub(super) fn advance_epoch(
        mut self,
        target_epoch: HadrEpoch,
    ) -> AndromedaResult<HadrMembershipSnapshot> {
        self.require_future_epoch(target_epoch)?;
        let previous_epoch = self.epoch;
        self.epoch = target_epoch;
        self.push_epoch_advanced(previous_epoch, target_epoch);
        Self::with_records(self.epoch, self.nodes, self.records)
    }

    pub(super) fn promote_primary(
        mut self,
        id: HadrNodeId,
        target_epoch: HadrEpoch,
        committed_safe_lsn: Lsn,
    ) -> AndromedaResult<HadrMembershipSnapshot> {
        self.require_future_epoch(target_epoch)?;
        if !self.contains(id) {
            return Err(storage_error("HADR membership node id is not registered"));
        }

        let previous_epoch = self.epoch;
        for node in &mut self.nodes {
            if node.id == id {
                if !node.role.is_promotion_eligible_role() {
                    return Err(storage_error(
                        "HADR membership node role is not promotion eligible",
                    ));
                }
                node.role = HadrNodeRole::Primary;
                node.role_epoch = target_epoch;
            } else if node.role == HadrNodeRole::Primary || node.role == HadrNodeRole::Candidate {
                node.role = HadrNodeRole::Replica;
                node.role_epoch = target_epoch;
            }
        }

        self.epoch = target_epoch;
        self.push_epoch_advanced(previous_epoch, target_epoch);
        self.records.push(HadrMembershipRecord::PrimaryPromoted {
            node_id: id,
            epoch: target_epoch,
            committed_safe_lsn,
        });
        Self::with_records(self.epoch, self.nodes, self.records)
    }

    pub(super) fn validate(&self) -> AndromedaResult<()> {
        membership_state_validation::validate_membership_snapshot(self)
    }

    fn next_epoch(&self) -> AndromedaResult<HadrEpoch> {
        self.epoch
            .checked_next()
            .ok_or_else(|| storage_error("HADR membership epoch overflow"))
    }

    fn require_future_epoch(&self, target_epoch: HadrEpoch) -> AndromedaResult<()> {
        if target_epoch <= self.epoch {
            return Err(storage_error(
                "HADR membership target epoch must advance current epoch",
            ));
        }
        Ok(())
    }

    fn push_epoch_advanced(&mut self, previous_epoch: HadrEpoch, new_epoch: HadrEpoch) {
        self.records.push(HadrMembershipRecord::EpochAdvanced {
            previous_epoch,
            new_epoch,
        });
    }
}
