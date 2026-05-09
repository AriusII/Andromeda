//! Durable HADR membership storage.
//!
//! The store owns only cluster membership and role/epoch metadata. It does not
//! participate in WAL recovery, page storage, or CLI dry-run contracts.

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Lsn;

use super::types::{HadrEpoch, HadrNodeId, HadrNodeRole};
use super::{
    membership_persistence_format::{decode_snapshot, encode_snapshot, read_membership_file},
    membership_state_validation,
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

    fn register_node(
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

    fn update_node_role(
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

    fn deregister_node(mut self, id: HadrNodeId) -> AndromedaResult<HadrMembershipSnapshot> {
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

    fn fence_node(mut self, id: HadrNodeId) -> AndromedaResult<HadrMembershipSnapshot> {
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

    fn advance_epoch(mut self, target_epoch: HadrEpoch) -> AndromedaResult<HadrMembershipSnapshot> {
        self.require_future_epoch(target_epoch)?;
        let previous_epoch = self.epoch;
        self.epoch = target_epoch;
        self.push_epoch_advanced(previous_epoch, target_epoch);
        Self::with_records(self.epoch, self.nodes, self.records)
    }

    fn promote_primary(
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

    fn validate(&self) -> AndromedaResult<()> {
        membership_state_validation::validate_membership_snapshot(self)
    }
}

/// Durable membership operations used by runtime storage code.
pub trait HadrMembershipStore {
    fn load(&self) -> AndromedaResult<Option<HadrMembershipSnapshot>>;

    fn register_node(
        &self,
        id: HadrNodeId,
        role: HadrNodeRole,
    ) -> AndromedaResult<HadrMembershipSnapshot>;

    fn update_node_role(
        &self,
        id: HadrNodeId,
        role: HadrNodeRole,
    ) -> AndromedaResult<HadrMembershipSnapshot>;

    fn deregister_node(&self, id: HadrNodeId) -> AndromedaResult<HadrMembershipSnapshot>;

    fn fence_node(&self, id: HadrNodeId) -> AndromedaResult<HadrMembershipSnapshot>;

    fn advance_epoch(&self, target_epoch: HadrEpoch) -> AndromedaResult<HadrMembershipSnapshot>;

    fn promote_primary(
        &self,
        id: HadrNodeId,
        target_epoch: HadrEpoch,
        committed_safe_lsn: Lsn,
    ) -> AndromedaResult<HadrMembershipSnapshot>;
}

/// File-backed HADR membership store using canonical bytes plus SHA-256.
#[derive(Debug, Clone)]
pub struct FileBackedHadrMembershipStore {
    path: PathBuf,
}

impl FileBackedHadrMembershipStore {
    pub fn open(path: impl Into<PathBuf>) -> AndromedaResult<Self> {
        let store = Self { path: path.into() };
        store.recover_interrupted_publish()?;
        if store.path.exists() {
            store
                .load()?
                .ok_or_else(|| storage_error("HADR membership file is missing"))?;
        }
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn tmp_path(&self) -> PathBuf {
        self.path.with_extension("tmp")
    }

    fn backup_path(&self) -> PathBuf {
        self.path.with_extension("bak")
    }

    fn recover_interrupted_publish(&self) -> AndromedaResult<()> {
        let tmp_path = self.tmp_path();
        let backup_path = self.backup_path();

        if !self.path.exists() && backup_path.exists() {
            fs::rename(&backup_path, &self.path)
                .map_err(|err| io_error("restore HADR membership backup file", err))?;
        }
        if self.path.exists() && backup_path.exists() {
            fs::remove_file(&backup_path)
                .map_err(|err| io_error("remove stale HADR membership backup file", err))?;
        }
        if self.path.exists() && tmp_path.exists() {
            fs::remove_file(&tmp_path)
                .map_err(|err| io_error("remove stale HADR membership temp file", err))?;
        }
        Ok(())
    }

    fn load_or_empty(&self) -> AndromedaResult<HadrMembershipSnapshot> {
        self.load()
            .map(|snapshot| snapshot.unwrap_or_else(HadrMembershipSnapshot::empty))
    }

    fn persist(&self, snapshot: &HadrMembershipSnapshot) -> AndromedaResult<()> {
        snapshot.validate()?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| io_error("create HADR membership directory", err))?;
        }

        let bytes = encode_snapshot(snapshot)?;
        let tmp_path = self.tmp_path();
        let backup_path = self.backup_path();
        let mut tmp = File::create(&tmp_path)
            .map_err(|err| io_error("create HADR membership temp file", err))?;
        tmp.write_all(&bytes)
            .map_err(|err| io_error("write HADR membership temp file", err))?;
        tmp.sync_all()
            .map_err(|err| io_error("sync HADR membership temp file", err))?;
        drop(tmp);

        if backup_path.exists() {
            fs::remove_file(&backup_path)
                .map_err(|err| io_error("remove stale HADR membership backup file", err))?;
        }
        if self.path.exists() {
            fs::rename(&self.path, &backup_path)
                .map_err(|err| io_error("backup HADR membership file", err))?;
        }
        if let Err(err) = fs::rename(&tmp_path, &self.path) {
            if backup_path.exists() && !self.path.exists() {
                let _ = fs::rename(&backup_path, &self.path);
            }
            return Err(io_error("rename HADR membership temp file", err));
        }

        if backup_path.exists() {
            let _ = fs::remove_file(&backup_path);
        }

        Ok(())
    }
}

impl HadrMembershipStore for FileBackedHadrMembershipStore {
    fn load(&self) -> AndromedaResult<Option<HadrMembershipSnapshot>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes = read_membership_file(&self.path)?;
        decode_snapshot(&bytes).map(Some)
    }

    fn register_node(
        &self,
        id: HadrNodeId,
        role: HadrNodeRole,
    ) -> AndromedaResult<HadrMembershipSnapshot> {
        let snapshot = self.load_or_empty()?.register_node(id, role)?;
        self.persist(&snapshot)?;
        Ok(snapshot)
    }

    fn update_node_role(
        &self,
        id: HadrNodeId,
        role: HadrNodeRole,
    ) -> AndromedaResult<HadrMembershipSnapshot> {
        let snapshot = self.load_or_empty()?.update_node_role(id, role)?;
        self.persist(&snapshot)?;
        Ok(snapshot)
    }

    fn deregister_node(&self, id: HadrNodeId) -> AndromedaResult<HadrMembershipSnapshot> {
        let snapshot = self.load_or_empty()?.deregister_node(id)?;
        self.persist(&snapshot)?;
        Ok(snapshot)
    }

    fn fence_node(&self, id: HadrNodeId) -> AndromedaResult<HadrMembershipSnapshot> {
        let snapshot = self.load_or_empty()?.fence_node(id)?;
        self.persist(&snapshot)?;
        Ok(snapshot)
    }

    fn advance_epoch(&self, target_epoch: HadrEpoch) -> AndromedaResult<HadrMembershipSnapshot> {
        let snapshot = self.load_or_empty()?.advance_epoch(target_epoch)?;
        self.persist(&snapshot)?;
        Ok(snapshot)
    }

    fn promote_primary(
        &self,
        id: HadrNodeId,
        target_epoch: HadrEpoch,
        committed_safe_lsn: Lsn,
    ) -> AndromedaResult<HadrMembershipSnapshot> {
        let snapshot =
            self.load_or_empty()?
                .promote_primary(id, target_epoch, committed_safe_lsn)?;
        self.persist(&snapshot)?;
        Ok(snapshot)
    }
}

fn io_error(action: &str, err: std::io::Error) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, format!("{action}: {err}"))
}

fn storage_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
