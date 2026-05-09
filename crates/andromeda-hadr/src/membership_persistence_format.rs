use std::{fs, path::Path};

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use sha2::{Digest, Sha256};

use crate::Lsn;

use super::{
    membership_state_validation::{
        HADR_MEMBERSHIP_MAX_NODES, HADR_MEMBERSHIP_MAX_RECORDS, validate_node_count,
        validate_record_count,
    },
    membership_store::{HadrMembershipNode, HadrMembershipRecord, HadrMembershipSnapshot},
    types::{HadrEpoch, HadrNodeId, HadrNodeRole},
};

const FORMAT_VERSION_V1: u64 = 1;
const FORMAT_VERSION_V2: u64 = 2;
const FILE_MAGIC: &[u8; 16] = b"ANDHADR-MSTORE\0\0";
const CHECKSUM_LEN: usize = 32;
const PAYLOAD_LEN_FIELD: usize = 8;
const HEADER_LEN: usize = FILE_MAGIC.len() + CHECKSUM_LEN + PAYLOAD_LEN_FIELD;
const PAYLOAD_V1_HEADER_LEN: usize = 24;
const PAYLOAD_V2_HEADER_LEN: usize = 32;
const NODE_ENTRY_LEN: usize = 24;
const RECORD_ENTRY_LEN: usize = 32;
const HADR_MEMBERSHIP_MAX_FILE_BYTES: u64 = 1024 * 1024;

pub(super) fn encode_snapshot(snapshot: &HadrMembershipSnapshot) -> AndromedaResult<Vec<u8>> {
    let payload = encode_payload(snapshot)?;
    let checksum = Sha256::digest(&payload);
    let capacity = HEADER_LEN
        .checked_add(payload.len())
        .ok_or_else(|| storage_error("HADR membership file length overflow"))?;
    let mut encoded = Vec::with_capacity(capacity);
    encoded.extend_from_slice(FILE_MAGIC);
    encoded.extend_from_slice(&checksum);
    let payload_len = u64::try_from(payload.len())
        .map_err(|_| storage_error("HADR membership payload length exceeds u64"))?;
    encoded.extend_from_slice(&payload_len.to_le_bytes());
    encoded.extend_from_slice(&payload);
    Ok(encoded)
}

pub(super) fn decode_snapshot(bytes: &[u8]) -> AndromedaResult<HadrMembershipSnapshot> {
    if bytes.len() < HEADER_LEN {
        return Err(storage_error("HADR membership file is truncated"));
    }
    if &bytes[..FILE_MAGIC.len()] != FILE_MAGIC {
        return Err(storage_error("HADR membership file magic mismatch"));
    }

    let checksum_start = FILE_MAGIC.len();
    let checksum_end = checksum_start + CHECKSUM_LEN;
    let payload_len_start = checksum_end;
    let payload_len_end = payload_len_start + PAYLOAD_LEN_FIELD;
    let payload_len = usize::try_from(read_u64(&bytes[payload_len_start..payload_len_end])?)
        .map_err(|_| storage_error("HADR membership payload length exceeds usize"))?;
    validate_payload_len(payload_len)?;
    let expected_len = HEADER_LEN
        .checked_add(payload_len)
        .ok_or_else(|| storage_error("HADR membership file length overflow"))?;
    if bytes.len() != expected_len {
        return Err(storage_error("HADR membership file length mismatch"));
    }

    let payload = &bytes[HEADER_LEN..];
    let computed = Sha256::digest(payload);
    let computed_checksum: &[u8] = computed.as_ref();
    if computed_checksum != &bytes[checksum_start..checksum_end] {
        return Err(storage_error("HADR membership file checksum mismatch"));
    }

    decode_payload(payload)
}

pub(super) fn read_membership_file(path: &Path) -> AndromedaResult<Vec<u8>> {
    let metadata = fs::metadata(path).map_err(|err| io_error("stat HADR membership file", err))?;
    if metadata.len() > HADR_MEMBERSHIP_MAX_FILE_BYTES {
        return Err(storage_error(
            "HADR membership file exceeds bounded read limit",
        ));
    }
    fs::read(path).map_err(|err| io_error("read HADR membership file", err))
}

fn encode_payload(snapshot: &HadrMembershipSnapshot) -> AndromedaResult<Vec<u8>> {
    validate_node_count(snapshot.nodes().len())?;
    validate_record_count(snapshot.records().len())?;
    let mut payload = Vec::with_capacity(membership_payload_capacity(
        snapshot.nodes().len(),
        snapshot.records().len(),
    )?);
    payload.extend_from_slice(&FORMAT_VERSION_V2.to_le_bytes());
    payload.extend_from_slice(&snapshot.epoch().get().to_le_bytes());
    let node_count = u64::try_from(snapshot.nodes().len())
        .map_err(|_| storage_error("HADR membership node count exceeds u64"))?;
    let record_count = u64::try_from(snapshot.records().len())
        .map_err(|_| storage_error("HADR membership record count exceeds u64"))?;
    payload.extend_from_slice(&node_count.to_le_bytes());
    payload.extend_from_slice(&record_count.to_le_bytes());
    for node in snapshot.nodes() {
        payload.extend_from_slice(&node.id.get().to_le_bytes());
        payload.push(role_to_tag(node.role));
        payload.extend_from_slice(&[0; 7]);
        payload.extend_from_slice(&node.role_epoch.get().to_le_bytes());
    }
    for record in snapshot.records() {
        encode_record(&mut payload, *record);
    }
    Ok(payload)
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
    let node_count = usize::try_from(cursor.read_u64()?)
        .map_err(|_| storage_error("HADR membership node count exceeds usize"))?;
    validate_node_count(node_count)?;
    let expected_len = PAYLOAD_V1_HEADER_LEN
        .checked_add(
            node_count
                .checked_mul(NODE_ENTRY_LEN)
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
    let node_count = usize::try_from(cursor.read_u64()?)
        .map_err(|_| storage_error("HADR membership node count exceeds usize"))?;
    let record_count = usize::try_from(cursor.read_u64()?)
        .map_err(|_| storage_error("HADR membership record count exceeds usize"))?;
    validate_node_count(node_count)?;
    validate_record_count(record_count)?;
    let expected_len = membership_payload_capacity(node_count, record_count)?;
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
        },
        HadrMembershipRecord::NodeDeregistered { node_id, epoch } => {
            payload.push(2);
            payload.push(0);
            payload.extend_from_slice(&[0; 6]);
            payload.extend_from_slice(&node_id.get().to_le_bytes());
            payload.extend_from_slice(&epoch.get().to_le_bytes());
            payload.extend_from_slice(&0u64.to_le_bytes());
        },
        HadrMembershipRecord::NodeFenced { node_id, epoch } => {
            payload.push(3);
            payload.push(0);
            payload.extend_from_slice(&[0; 6]);
            payload.extend_from_slice(&node_id.get().to_le_bytes());
            payload.extend_from_slice(&epoch.get().to_le_bytes());
            payload.extend_from_slice(&0u64.to_le_bytes());
        },
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
        },
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
        },
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
        },
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
        },
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
        },
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
        },
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
        },
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
        },
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
        },
        _ => Err(storage_error("HADR membership record tag is invalid")),
    }
}

fn read_u64(bytes: &[u8]) -> AndromedaResult<u64> {
    let array: [u8; 8] = bytes
        .try_into()
        .map_err(|_| storage_error("HADR membership u64 field is truncated"))?;
    Ok(u64::from_le_bytes(array))
}

fn validate_payload_len(len: usize) -> AndromedaResult<()> {
    let max_len =
        membership_payload_capacity(HADR_MEMBERSHIP_MAX_NODES, HADR_MEMBERSHIP_MAX_RECORDS)?;
    if len > max_len {
        return Err(storage_error(
            "HADR membership payload length exceeds bounded limit",
        ));
    }
    Ok(())
}

fn membership_payload_capacity(node_count: usize, record_count: usize) -> AndromedaResult<usize> {
    PAYLOAD_V2_HEADER_LEN
        .checked_add(
            node_count
                .checked_mul(NODE_ENTRY_LEN)
                .ok_or_else(|| storage_error("HADR membership node count overflow"))?,
        )
        .and_then(|len| {
            record_count
                .checked_mul(RECORD_ENTRY_LEN)
                .and_then(|records_len| len.checked_add(records_len))
        })
        .ok_or_else(|| storage_error("HADR membership payload length overflow"))
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
