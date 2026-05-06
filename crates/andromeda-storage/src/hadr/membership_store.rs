//! Durable HADR membership storage.
//!
//! The store owns only cluster membership and role/epoch metadata. It does not
//! participate in WAL recovery, page storage, or CLI dry-run contracts.

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use sha2::{Digest, Sha256};

use crate::Lsn;

use super::types::{HadrEpoch, HadrNodeId, HadrNodeRole};

const FORMAT_VERSION_V1: u64 = 1;
const FORMAT_VERSION_V2: u64 = 2;
const FILE_MAGIC: &[u8; 16] = b"ANDHADR-MSTORE\0\0";
const HEADER_LEN: usize = FILE_MAGIC.len() + 32 + 8;

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
        let next_epoch = self.next_epoch()?;
        let previous_epoch = self.epoch;
        let Some(node) = self.nodes.iter_mut().find(|node| node.id == id) else {
            return Err(storage_error("HADR membership node id is not registered"));
        };

        node.role = role;
        node.role_epoch = next_epoch;
        self.epoch = next_epoch;
        self.push_epoch_advanced(previous_epoch, next_epoch);
        if role == HadrNodeRole::Primary {
            self.records.push(HadrMembershipRecord::PrimaryPromoted {
                node_id: id,
                epoch: next_epoch,
                committed_safe_lsn: Lsn::ZERO,
            });
        } else {
            self.records.push(HadrMembershipRecord::NodeRoleUpdated {
                node_id: id,
                role,
                epoch: next_epoch,
            });
        }
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
        for node in &self.nodes {
            if node.id.is_zero() {
                return Err(storage_error("HADR membership node id must not be zero"));
            }
            if node.role_epoch > self.epoch {
                return Err(storage_error(
                    "HADR membership node role epoch exceeds snapshot epoch",
                ));
            }
        }

        for record in &self.records {
            if record.epoch() > self.epoch {
                return Err(storage_error(
                    "HADR membership record epoch exceeds snapshot epoch",
                ));
            }
            if let HadrMembershipRecord::EpochAdvanced {
                previous_epoch,
                new_epoch,
            } = *record
                && previous_epoch >= new_epoch
            {
                return Err(storage_error(
                    "HADR membership epoch advance record is not monotonic",
                ));
            }
        }

        for pair in self.nodes.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(storage_error("HADR membership contains duplicate node ids"));
            }
        }

        let primary_count = self
            .nodes
            .iter()
            .filter(|node| node.role == HadrNodeRole::Primary)
            .count();
        if primary_count > 1 {
            return Err(storage_error(
                "HADR membership cannot contain two primaries in one snapshot",
            ));
        }

        self.validate_record_replay()?;

        Ok(())
    }

    fn validate_record_replay(&self) -> AndromedaResult<()> {
        if self.records.is_empty() {
            return Ok(());
        }

        let mut replay_epoch = HadrEpoch::ZERO;
        let mut replay_nodes = Vec::new();
        let mut action_seen_for_epoch = false;

        for record in &self.records {
            match *record {
                HadrMembershipRecord::EpochAdvanced {
                    previous_epoch,
                    new_epoch,
                } => {
                    if previous_epoch != replay_epoch {
                        return Err(storage_error(
                            "HADR membership epoch advance record does not follow replay epoch",
                        ));
                    }
                    if previous_epoch >= new_epoch {
                        return Err(storage_error(
                            "HADR membership epoch advance record is not monotonic",
                        ));
                    }
                    replay_epoch = new_epoch;
                    action_seen_for_epoch = false;
                }
                HadrMembershipRecord::NodeRegistered {
                    node_id,
                    role,
                    epoch,
                } => {
                    validate_action_epoch(epoch, replay_epoch, action_seen_for_epoch)?;
                    action_seen_for_epoch = true;
                    if node_id.is_zero() {
                        return Err(storage_error("HADR membership node id must not be zero"));
                    }
                    if replay_nodes
                        .iter()
                        .any(|node: &HadrMembershipNode| node.id == node_id)
                    {
                        return Err(storage_error("HADR membership contains duplicate node ids"));
                    }
                    if role == HadrNodeRole::Primary
                        && replay_nodes
                            .iter()
                            .any(|node| node.role == HadrNodeRole::Primary)
                    {
                        return Err(storage_error(
                            "HADR membership cannot contain two primaries in one replay",
                        ));
                    }
                    replay_nodes.push(HadrMembershipNode::new(node_id, role, epoch));
                }
                HadrMembershipRecord::NodeDeregistered { node_id, epoch } => {
                    validate_action_epoch(epoch, replay_epoch, action_seen_for_epoch)?;
                    action_seen_for_epoch = true;
                    let original_len = replay_nodes.len();
                    replay_nodes.retain(|node| node.id != node_id);
                    if replay_nodes.len() == original_len {
                        return Err(storage_error(
                            "HADR membership deregister record references an unregistered node",
                        ));
                    }
                }
                HadrMembershipRecord::NodeFenced { node_id, epoch } => {
                    validate_action_epoch(epoch, replay_epoch, action_seen_for_epoch)?;
                    action_seen_for_epoch = true;
                    let Some(node) = replay_nodes.iter_mut().find(|node| node.id == node_id) else {
                        return Err(storage_error(
                            "HADR membership fence record references an unregistered node",
                        ));
                    };
                    if node.role != HadrNodeRole::Replica {
                        node.role = HadrNodeRole::Replica;
                        node.role_epoch = epoch;
                    }
                }
                HadrMembershipRecord::NodeRoleUpdated {
                    node_id,
                    role,
                    epoch,
                } => {
                    validate_action_epoch(epoch, replay_epoch, action_seen_for_epoch)?;
                    action_seen_for_epoch = true;
                    if role == HadrNodeRole::Primary
                        && replay_nodes
                            .iter()
                            .any(|node| node.id != node_id && node.role == HadrNodeRole::Primary)
                    {
                        return Err(storage_error(
                            "HADR membership cannot contain two primaries in one replay",
                        ));
                    }
                    let Some(node) = replay_nodes.iter_mut().find(|node| node.id == node_id) else {
                        return Err(storage_error(
                            "HADR membership role record references an unregistered node",
                        ));
                    };
                    node.role = role;
                    node.role_epoch = epoch;
                }
                HadrMembershipRecord::PrimaryPromoted {
                    node_id,
                    epoch,
                    committed_safe_lsn: _,
                } => {
                    validate_action_epoch(epoch, replay_epoch, action_seen_for_epoch)?;
                    action_seen_for_epoch = true;
                    if !replay_nodes.iter().any(|node| node.id == node_id) {
                        return Err(storage_error(
                            "HADR membership promote record references an unregistered node",
                        ));
                    }
                    for node in &mut replay_nodes {
                        if node.id == node_id {
                            if !node.role.is_promotion_eligible_role() {
                                return Err(storage_error(
                                    "HADR membership promote record references an ineligible node",
                                ));
                            }
                            node.role = HadrNodeRole::Primary;
                            node.role_epoch = epoch;
                        } else if node.role == HadrNodeRole::Primary
                            || node.role == HadrNodeRole::Candidate
                        {
                            node.role = HadrNodeRole::Replica;
                            node.role_epoch = epoch;
                        }
                    }
                }
            }
        }

        replay_nodes.sort_by_key(|node| node.id);
        if replay_epoch != self.epoch {
            return Err(storage_error(
                "HADR membership record replay epoch does not match snapshot epoch",
            ));
        }
        if replay_nodes != self.nodes {
            return Err(storage_error(
                "HADR membership record replay does not match snapshot nodes",
            ));
        }

        Ok(())
    }
}

fn validate_action_epoch(
    action_epoch: HadrEpoch,
    replay_epoch: HadrEpoch,
    action_seen_for_epoch: bool,
) -> AndromedaResult<()> {
    if action_epoch != replay_epoch || replay_epoch.is_zero() {
        return Err(storage_error(
            "HADR membership action record must follow its epoch advance",
        ));
    }
    if action_seen_for_epoch {
        return Err(storage_error(
            "HADR membership epoch contains more than one action record",
        ));
    }
    Ok(())
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

        let bytes = encode_snapshot(snapshot);
        let tmp_path = self.path.with_extension("tmp");
        let mut tmp = File::create(&tmp_path)
            .map_err(|err| io_error("create HADR membership temp file", err))?;
        tmp.write_all(&bytes)
            .map_err(|err| io_error("write HADR membership temp file", err))?;
        tmp.sync_all()
            .map_err(|err| io_error("sync HADR membership temp file", err))?;
        drop(tmp);

        if self.path.exists() {
            fs::remove_file(&self.path)
                .map_err(|err| io_error("replace HADR membership file", err))?;
        }
        fs::rename(&tmp_path, &self.path)
            .map_err(|err| io_error("rename HADR membership temp file", err))?;

        Ok(())
    }
}

impl HadrMembershipStore for FileBackedHadrMembershipStore {
    fn load(&self) -> AndromedaResult<Option<HadrMembershipSnapshot>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes =
            fs::read(&self.path).map_err(|err| io_error("read HADR membership file", err))?;
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

fn encode_snapshot(snapshot: &HadrMembershipSnapshot) -> Vec<u8> {
    let payload = encode_payload(snapshot);
    let checksum = Sha256::digest(&payload);
    let mut encoded = Vec::with_capacity(HEADER_LEN + payload.len());
    encoded.extend_from_slice(FILE_MAGIC);
    encoded.extend_from_slice(&checksum);
    encoded.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    encoded.extend_from_slice(&payload);
    encoded
}

fn decode_snapshot(bytes: &[u8]) -> AndromedaResult<HadrMembershipSnapshot> {
    if bytes.len() < HEADER_LEN {
        return Err(storage_error("HADR membership file is truncated"));
    }
    if &bytes[..FILE_MAGIC.len()] != FILE_MAGIC {
        return Err(storage_error("HADR membership file magic mismatch"));
    }

    let checksum_start = FILE_MAGIC.len();
    let checksum_end = checksum_start + 32;
    let payload_len_start = checksum_end;
    let payload_len_end = payload_len_start + 8;
    let payload_len = read_u64(&bytes[payload_len_start..payload_len_end])? as usize;
    let expected_len = HEADER_LEN
        .checked_add(payload_len)
        .ok_or_else(|| storage_error("HADR membership file length overflow"))?;
    if bytes.len() != expected_len {
        return Err(storage_error("HADR membership file length mismatch"));
    }

    let payload = &bytes[HEADER_LEN..];
    let computed = Sha256::digest(payload);
    if computed.as_slice() != &bytes[checksum_start..checksum_end] {
        return Err(storage_error("HADR membership file checksum mismatch"));
    }

    decode_payload(payload)
}

fn encode_payload(snapshot: &HadrMembershipSnapshot) -> Vec<u8> {
    let mut payload =
        Vec::with_capacity(32 + snapshot.nodes.len() * 24 + snapshot.records.len() * 32);
    payload.extend_from_slice(&FORMAT_VERSION_V2.to_le_bytes());
    payload.extend_from_slice(&snapshot.epoch.get().to_le_bytes());
    payload.extend_from_slice(&(snapshot.nodes.len() as u64).to_le_bytes());
    payload.extend_from_slice(&(snapshot.records.len() as u64).to_le_bytes());
    for node in &snapshot.nodes {
        payload.extend_from_slice(&node.id.get().to_le_bytes());
        payload.push(role_to_tag(node.role));
        payload.extend_from_slice(&[0; 7]);
        payload.extend_from_slice(&node.role_epoch.get().to_le_bytes());
    }
    for record in &snapshot.records {
        encode_record(&mut payload, *record);
    }
    payload
}

fn decode_payload(payload: &[u8]) -> AndromedaResult<HadrMembershipSnapshot> {
    let mut cursor = PayloadCursor::new(payload);
    let version = cursor.read_u64()?;
    match version {
        FORMAT_VERSION_V1 => decode_payload_v1(payload, cursor),
        FORMAT_VERSION_V2 => decode_payload_v2(payload, cursor),
        _ => Err(storage_error("HADR membership file version is unsupported")),
    }
}

fn decode_payload_v1(
    payload: &[u8],
    mut cursor: PayloadCursor<'_>,
) -> AndromedaResult<HadrMembershipSnapshot> {
    let epoch = HadrEpoch::new(cursor.read_u64()?);
    let node_count = cursor.read_u64()? as usize;
    let expected_len = 24usize
        .checked_add(
            node_count
                .checked_mul(24)
                .ok_or_else(|| storage_error("HADR membership node count overflow"))?,
        )
        .ok_or_else(|| storage_error("HADR membership payload length overflow"))?;
    if payload.len() != expected_len {
        return Err(storage_error("HADR membership payload length mismatch"));
    }

    let mut nodes = Vec::with_capacity(node_count);
    for _ in 0..node_count {
        let id = HadrNodeId::new(cursor.read_u64()?);
        let role = tag_to_role(cursor.read_u8()?)?;
        cursor.read_padding(7)?;
        let role_epoch = HadrEpoch::new(cursor.read_u64()?);
        nodes.push(HadrMembershipNode::new(id, role, role_epoch));
    }

    HadrMembershipSnapshot::new(epoch, nodes)
}

fn decode_payload_v2(
    payload: &[u8],
    mut cursor: PayloadCursor<'_>,
) -> AndromedaResult<HadrMembershipSnapshot> {
    let epoch = HadrEpoch::new(cursor.read_u64()?);
    let node_count = cursor.read_u64()? as usize;
    let record_count = cursor.read_u64()? as usize;
    let nodes_len = node_count
        .checked_mul(24)
        .ok_or_else(|| storage_error("HADR membership node count overflow"))?;
    let records_len = record_count
        .checked_mul(32)
        .ok_or_else(|| storage_error("HADR membership record count overflow"))?;
    let expected_len = 32usize
        .checked_add(nodes_len)
        .and_then(|len| len.checked_add(records_len))
        .ok_or_else(|| storage_error("HADR membership payload length overflow"))?;
    if payload.len() != expected_len {
        return Err(storage_error("HADR membership payload length mismatch"));
    }

    let mut nodes = Vec::with_capacity(node_count);
    for _ in 0..node_count {
        let id = HadrNodeId::new(cursor.read_u64()?);
        let role = tag_to_role(cursor.read_u8()?)?;
        cursor.read_padding(7)?;
        let role_epoch = HadrEpoch::new(cursor.read_u64()?);
        nodes.push(HadrMembershipNode::new(id, role, role_epoch));
    }

    let mut records = Vec::with_capacity(record_count);
    for _ in 0..record_count {
        records.push(decode_record(&mut cursor)?);
    }

    HadrMembershipSnapshot::with_records(epoch, nodes, records)
}

fn role_to_tag(role: HadrNodeRole) -> u8 {
    match role {
        HadrNodeRole::Primary => 1,
        HadrNodeRole::Replica => 2,
        HadrNodeRole::Candidate => 3,
    }
}

fn tag_to_role(tag: u8) -> AndromedaResult<HadrNodeRole> {
    match tag {
        1 => Ok(HadrNodeRole::Primary),
        2 => Ok(HadrNodeRole::Replica),
        3 => Ok(HadrNodeRole::Candidate),
        _ => Err(storage_error("HADR membership node role is invalid")),
    }
}

fn encode_record(payload: &mut Vec<u8>, record: HadrMembershipRecord) {
    match record {
        HadrMembershipRecord::NodeRegistered {
            node_id,
            role,
            epoch,
        } => {
            payload.push(1);
            payload.push(role_to_tag(role));
            payload.extend_from_slice(&[0; 6]);
            payload.extend_from_slice(&node_id.get().to_le_bytes());
            payload.extend_from_slice(&epoch.get().to_le_bytes());
            payload.extend_from_slice(&0u64.to_le_bytes());
        }
        HadrMembershipRecord::NodeDeregistered { node_id, epoch } => {
            payload.push(2);
            payload.push(0);
            payload.extend_from_slice(&[0; 6]);
            payload.extend_from_slice(&node_id.get().to_le_bytes());
            payload.extend_from_slice(&epoch.get().to_le_bytes());
            payload.extend_from_slice(&0u64.to_le_bytes());
        }
        HadrMembershipRecord::NodeFenced { node_id, epoch } => {
            payload.push(3);
            payload.push(0);
            payload.extend_from_slice(&[0; 6]);
            payload.extend_from_slice(&node_id.get().to_le_bytes());
            payload.extend_from_slice(&epoch.get().to_le_bytes());
            payload.extend_from_slice(&0u64.to_le_bytes());
        }
        HadrMembershipRecord::EpochAdvanced {
            previous_epoch,
            new_epoch,
        } => {
            payload.push(4);
            payload.push(0);
            payload.extend_from_slice(&[0; 6]);
            payload.extend_from_slice(&previous_epoch.get().to_le_bytes());
            payload.extend_from_slice(&new_epoch.get().to_le_bytes());
            payload.extend_from_slice(&0u64.to_le_bytes());
        }
        HadrMembershipRecord::NodeRoleUpdated {
            node_id,
            role,
            epoch,
        } => {
            payload.push(6);
            payload.push(role_to_tag(role));
            payload.extend_from_slice(&[0; 6]);
            payload.extend_from_slice(&node_id.get().to_le_bytes());
            payload.extend_from_slice(&epoch.get().to_le_bytes());
            payload.extend_from_slice(&0u64.to_le_bytes());
        }
        HadrMembershipRecord::PrimaryPromoted {
            node_id,
            epoch,
            committed_safe_lsn,
        } => {
            payload.push(5);
            payload.push(0);
            payload.extend_from_slice(&[0; 6]);
            payload.extend_from_slice(&node_id.get().to_le_bytes());
            payload.extend_from_slice(&epoch.get().to_le_bytes());
            payload.extend_from_slice(&committed_safe_lsn.get().to_le_bytes());
        }
    }
}

fn decode_record(cursor: &mut PayloadCursor<'_>) -> AndromedaResult<HadrMembershipRecord> {
    let tag = cursor.read_u8()?;
    let aux = cursor.read_u8()?;
    cursor.read_padding(6)?;
    let a = cursor.read_u64()?;
    let b = cursor.read_u64()?;
    let c = cursor.read_u64()?;

    match tag {
        1 => {
            if c != 0 {
                return Err(storage_error(
                    "HADR membership registered record padding is invalid",
                ));
            }
            Ok(HadrMembershipRecord::NodeRegistered {
                node_id: HadrNodeId::new(a),
                role: tag_to_role(aux)?,
                epoch: HadrEpoch::new(b),
            })
        }
        2 => {
            if aux != 0 || c != 0 {
                return Err(storage_error(
                    "HADR membership deregister record padding is invalid",
                ));
            }
            Ok(HadrMembershipRecord::NodeDeregistered {
                node_id: HadrNodeId::new(a),
                epoch: HadrEpoch::new(b),
            })
        }
        3 => {
            if aux != 0 || c != 0 {
                return Err(storage_error(
                    "HADR membership fenced record padding is invalid",
                ));
            }
            Ok(HadrMembershipRecord::NodeFenced {
                node_id: HadrNodeId::new(a),
                epoch: HadrEpoch::new(b),
            })
        }
        4 => {
            if aux != 0 || c != 0 {
                return Err(storage_error(
                    "HADR membership epoch record padding is invalid",
                ));
            }
            Ok(HadrMembershipRecord::EpochAdvanced {
                previous_epoch: HadrEpoch::new(a),
                new_epoch: HadrEpoch::new(b),
            })
        }
        5 => {
            if aux != 0 {
                return Err(storage_error(
                    "HADR membership primary promoted record padding is invalid",
                ));
            }
            Ok(HadrMembershipRecord::PrimaryPromoted {
                node_id: HadrNodeId::new(a),
                epoch: HadrEpoch::new(b),
                committed_safe_lsn: Lsn::new(c),
            })
        }
        6 => {
            if c != 0 {
                return Err(storage_error(
                    "HADR membership role updated record padding is invalid",
                ));
            }
            Ok(HadrMembershipRecord::NodeRoleUpdated {
                node_id: HadrNodeId::new(a),
                role: tag_to_role(aux)?,
                epoch: HadrEpoch::new(b),
            })
        }
        _ => Err(storage_error("HADR membership record tag is invalid")),
    }
}

fn read_u64(bytes: &[u8]) -> AndromedaResult<u64> {
    let array: [u8; 8] = bytes
        .try_into()
        .map_err(|_| storage_error("HADR membership u64 field is truncated"))?;
    Ok(u64::from_le_bytes(array))
}

struct PayloadCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> PayloadCursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_u64(&mut self) -> AndromedaResult<u64> {
        let end = self
            .offset
            .checked_add(8)
            .ok_or_else(|| storage_error("HADR membership payload offset overflow"))?;
        if end > self.bytes.len() {
            return Err(storage_error("HADR membership payload is truncated"));
        }
        let value = read_u64(&self.bytes[self.offset..end])?;
        self.offset = end;
        Ok(value)
    }

    fn read_u8(&mut self) -> AndromedaResult<u8> {
        if self.offset >= self.bytes.len() {
            return Err(storage_error("HADR membership payload is truncated"));
        }
        let value = self.bytes[self.offset];
        self.offset += 1;
        Ok(value)
    }

    fn read_padding(&mut self, len: usize) -> AndromedaResult<()> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| storage_error("HADR membership payload offset overflow"))?;
        if end > self.bytes.len() {
            return Err(storage_error("HADR membership payload is truncated"));
        }
        if self.bytes[self.offset..end].iter().any(|byte| *byte != 0) {
            return Err(storage_error("HADR membership payload padding is invalid"));
        }
        self.offset = end;
        Ok(())
    }
}

fn io_error(action: &str, err: std::io::Error) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, format!("{action}: {err}"))
}

fn storage_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
