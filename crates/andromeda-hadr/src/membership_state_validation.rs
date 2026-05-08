use andromeda_core::AndromedaResult;

use super::{
    membership_store::{HadrMembershipNode, HadrMembershipRecord, HadrMembershipSnapshot},
    types::{HadrEpoch, HadrNodeRole},
};

pub(super) const HADR_MEMBERSHIP_MAX_NODES: usize = 1_024;
pub(super) const HADR_MEMBERSHIP_MAX_RECORDS: usize = 8_192;

pub(super) fn validate_membership_snapshot(
    snapshot: &HadrMembershipSnapshot,
) -> AndromedaResult<()> {
    validate_node_count(snapshot.nodes().len())?;
    validate_record_count(snapshot.records().len())?;
    validate_node_state(snapshot)?;
    validate_record_epochs(snapshot)?;
    validate_node_identity(snapshot)?;
    validate_record_replay(snapshot)?;
    Ok(())
}

pub(super) fn validate_node_count(count: usize) -> AndromedaResult<()> {
    if count > HADR_MEMBERSHIP_MAX_NODES {
        return Err(storage_error(
            "HADR membership node count exceeds bounded limit",
        ));
    }
    Ok(())
}

pub(super) fn validate_record_count(count: usize) -> AndromedaResult<()> {
    if count > HADR_MEMBERSHIP_MAX_RECORDS {
        return Err(storage_error(
            "HADR membership record count exceeds bounded limit",
        ));
    }
    Ok(())
}

fn validate_node_state(snapshot: &HadrMembershipSnapshot) -> AndromedaResult<()> {
    for node in snapshot.nodes() {
        if node.id.is_zero() {
            return Err(storage_error("HADR membership node id must not be zero"));
        }
        if node.role_epoch > snapshot.epoch() {
            return Err(storage_error(
                "HADR membership node role epoch exceeds snapshot epoch",
            ));
        }
    }
    Ok(())
}

fn validate_record_epochs(snapshot: &HadrMembershipSnapshot) -> AndromedaResult<()> {
    for record in snapshot.records() {
        if record.epoch() > snapshot.epoch() {
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
    Ok(())
}

fn validate_node_identity(snapshot: &HadrMembershipSnapshot) -> AndromedaResult<()> {
    for pair in snapshot.nodes().windows(2) {
        if pair[0].id == pair[1].id {
            return Err(storage_error("HADR membership contains duplicate node ids"));
        }
    }

    let primary_count = snapshot
        .nodes()
        .iter()
        .filter(|node| node.role == HadrNodeRole::Primary)
        .count();
    if primary_count > 1 {
        return Err(storage_error(
            "HADR membership cannot contain two primaries in one snapshot",
        ));
    }
    Ok(())
}

fn validate_record_replay(snapshot: &HadrMembershipSnapshot) -> AndromedaResult<()> {
    if snapshot.records().is_empty() {
        return Ok(());
    }

    let mut replay_epoch = HadrEpoch::ZERO;
    let mut replay_nodes = Vec::new();
    let mut action_seen_for_epoch = false;

    for record in snapshot.records() {
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
            },
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
            },
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
            },
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
            },
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
            },
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
            },
        }
    }

    replay_nodes.sort_by_key(|node| node.id);
    if replay_epoch != snapshot.epoch() {
        return Err(storage_error(
            "HADR membership record replay epoch does not match snapshot epoch",
        ));
    }
    if replay_nodes.as_slice() != snapshot.nodes() {
        return Err(storage_error(
            "HADR membership record replay does not match snapshot nodes",
        ));
    }

    Ok(())
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

fn storage_error(message: &'static str) -> andromeda_core::AndromedaError {
    andromeda_core::AndromedaError::new(andromeda_core::AndromedaErrorKind::Storage, message)
}
