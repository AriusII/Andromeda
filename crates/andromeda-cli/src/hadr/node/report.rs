use andromeda_core::AndromedaResult;
use andromeda_storage::{HadrNodeId, HadrQuorumMembership};

use crate::hadr::types::{NodeManagementReport, NodeMembershipMemberReport};

pub(in crate::hadr::node) struct NodeReportInput<'a> {
    pub(in crate::hadr::node) contract_preview: bool,
    pub(in crate::hadr::node) durable_backend: bool,
    pub(in crate::hadr::node) action: &'a str,
    pub(in crate::hadr::node) node_id: Option<u64>,
    pub(in crate::hadr::node) role: Option<&'a str>,
    pub(in crate::hadr::node) membership_epoch: Option<u64>,
    pub(in crate::hadr::node) members: Vec<NodeMembershipMemberReport>,
    pub(in crate::hadr::node) dry_run: bool,
    pub(in crate::hadr::node) would_apply: bool,
    pub(in crate::hadr::node) quorum_check: &'a str,
    pub(in crate::hadr::node) fencing_check: &'a str,
    pub(in crate::hadr::node) audit_event: &'a str,
    pub(in crate::hadr::node) required_permissions: &'a [&'a str],
    pub(in crate::hadr::node) failure_mode: Option<&'a str>,
    pub(in crate::hadr::node) message: &'a str,
}

pub(in crate::hadr::node) fn build_node_report(
    input: NodeReportInput<'_>,
) -> AndromedaResult<NodeManagementReport> {
    if let Some(node_id) = input.node_id {
        let membership = HadrQuorumMembership::new(vec![HadrNodeId::new(node_id)])?;
        debug_assert!(membership.contains(HadrNodeId::new(node_id)));
    }

    Ok(NodeManagementReport {
        contract_preview: input.contract_preview,
        durable_backend: input.durable_backend,
        action: input.action.to_string(),
        node_id: input.node_id,
        role: input.role.map(str::to_string),
        membership_epoch: input.membership_epoch,
        members: input.members,
        dry_run: input.dry_run,
        would_apply: input.would_apply,
        quorum_check: input.quorum_check.to_string(),
        fencing_check: input.fencing_check.to_string(),
        audit_event: input.audit_event.to_string(),
        required_permissions: input
            .required_permissions
            .iter()
            .map(|permission| (*permission).to_string())
            .collect(),
        failure_mode: input.failure_mode.map(str::to_string),
        message: input.message.to_string(),
    })
}
