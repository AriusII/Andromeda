use andromeda_error::AndromedaResult;

use crate::{
    membership_state_validation::{
        HADR_MEMBERSHIP_MAX_NODES, HADR_MEMBERSHIP_MAX_RECORDS, validate_node_count,
        validate_record_count,
    },
    membership_store::{HadrMembershipNode, HadrMembershipSnapshot},
    types::{HadrEpoch, HadrNodeId},
};

use super::{
    FORMAT_VERSION_V1, FORMAT_VERSION_V2, NODE_ENTRY_LEN, PAYLOAD_V1_HEADER_LEN,
    cursor::PayloadCursor,
    membership_payload_capacity,
    record::{decode_record, encode_record, role_to_tag, tag_to_role},
    storage_error,
};

pub(super) fn encode_payload(snapshot: &HadrMembershipSnapshot) -> AndromedaResult<Vec<u8>> {
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

pub(super) fn decode_payload(payload: &[u8]) -> AndromedaResult<HadrMembershipSnapshot> {
    let mut cursor = PayloadCursor::new(payload);
    let version = cursor.read_u64()?;
    match version {
        FORMAT_VERSION_V1 => decode_payload_v1(payload, cursor),
        FORMAT_VERSION_V2 => decode_payload_v2(payload, cursor),
        _ => Err(storage_error("HADR membership file version is unsupported")),
    }
}

pub(super) fn validate_payload_len(len: usize) -> AndromedaResult<()> {
    let max_len =
        membership_payload_capacity(HADR_MEMBERSHIP_MAX_NODES, HADR_MEMBERSHIP_MAX_RECORDS)?;
    if len > max_len {
        return Err(storage_error(
            "HADR membership payload length exceeds bounded limit",
        ));
    }
    Ok(())
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
