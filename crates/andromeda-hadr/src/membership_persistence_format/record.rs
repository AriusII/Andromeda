use andromeda_error::AndromedaResult;

use crate::{
    Lsn,
    membership_store::HadrMembershipRecord,
    types::{HadrEpoch, HadrNodeId, HadrNodeRole},
};

use super::{cursor::PayloadCursor, storage_error};

pub(super) fn role_to_tag(role: HadrNodeRole) -> u8 {
    match role {
        HadrNodeRole::Primary => 1,
        HadrNodeRole::Replica => 2,
        HadrNodeRole::Candidate => 3,
    }
}

pub(super) fn tag_to_role(tag: u8) -> AndromedaResult<HadrNodeRole> {
    match tag {
        1 => Ok(HadrNodeRole::Primary),
        2 => Ok(HadrNodeRole::Replica),
        3 => Ok(HadrNodeRole::Candidate),
        _ => Err(storage_error("HADR membership node role is invalid")),
    }
}

pub(super) fn encode_record(payload: &mut Vec<u8>, record: HadrMembershipRecord) {
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

pub(super) fn decode_record(
    cursor: &mut PayloadCursor<'_>,
) -> AndromedaResult<HadrMembershipRecord> {
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
