use std::path::Path;

use andromeda_core::AndromedaResult;
use andromeda_storage::{HadrMembershipStore, HadrNodeId, HadrNodeRole};

use crate::error::cli_error;
use crate::hadr::output::print_node_report;
use crate::hadr::runtime::{
    load_membership_snapshot, member_reports, membership_epoch, open_membership_store, role_label,
};

use super::options::{NodeDeregisterOptions, NodeFenceOptions, NodeRegisterOptions};
use super::report::{NodeReportInput, build_node_report};

pub(in crate::hadr::node) fn run_durable_node_register(
    options: &NodeRegisterOptions,
    path: &Path,
) -> AndromedaResult<()> {
    let node_id = HadrNodeId::new(options.node_id);
    let role = HadrNodeRole::Replica;

    if options.dry_run {
        let snapshot = load_membership_snapshot(path)?;
        if snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.contains(node_id))
        {
            return Err(cli_error(
                "hadr node register target node is already registered",
            ));
        }
        let report = build_node_report(NodeReportInput {
            contract_preview: false,
            durable_backend: true,
            action: "register",
            node_id: Some(options.node_id),
            role: Some(role_label(role)),
            membership_epoch: Some(membership_epoch(snapshot.as_ref())),
            members: member_reports(snapshot.as_ref()),
            dry_run: true,
            would_apply: true,
            quorum_check: "durable_membership_snapshot_validated",
            fencing_check: "not_applicable_for_replica_registration",
            audit_event: "hadr.node.register.requested",
            required_permissions: &["UpdateClusterManifest"],
            failure_mode: None,
            message: "dry-run accepted: durable runtime membership store would register replica node",
        })?;
        print_node_report(&report, options.json_output);
        return Ok(());
    }
    if !options.apply {
        return Err(cli_error(
            "hadr node register with --membership-store requires --apply or --dry-run",
        ));
    }

    let store = open_membership_store(path)?;
    let snapshot = store.register_node(node_id, role)?;
    let report = build_node_report(NodeReportInput {
        contract_preview: false,
        durable_backend: true,
        action: "register",
        node_id: Some(options.node_id),
        role: Some(role_label(role)),
        membership_epoch: Some(snapshot.epoch().get()),
        members: member_reports(Some(&snapshot)),
        dry_run: false,
        would_apply: true,
        quorum_check: "durable_membership_snapshot_updated",
        fencing_check: "not_applicable_for_replica_registration",
        audit_event: "hadr.node.register.applied",
        required_permissions: &["UpdateClusterManifest"],
        failure_mode: None,
        message: "durable HADR membership store updated with replica node",
    })?;
    print_node_report(&report, options.json_output);
    Ok(())
}

pub(in crate::hadr::node) fn run_durable_node_deregister(
    options: &NodeDeregisterOptions,
    path: &Path,
) -> AndromedaResult<()> {
    if !options.has_fencing_evidence {
        return Err(cli_error(
            "hadr node deregister requires --fencing-evidence <evidence-id>",
        ));
    }
    let node_id = HadrNodeId::new(options.node_id);

    if options.dry_run {
        let snapshot = load_membership_snapshot(path)?;
        if !snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.contains(node_id))
        {
            return Err(cli_error(
                "hadr node deregister target node is not registered",
            ));
        }
        let report = build_node_report(NodeReportInput {
            contract_preview: false,
            durable_backend: true,
            action: "deregister",
            node_id: Some(options.node_id),
            role: None,
            membership_epoch: Some(membership_epoch(snapshot.as_ref())),
            members: member_reports(snapshot.as_ref()),
            dry_run: true,
            would_apply: true,
            quorum_check: "durable_membership_snapshot_validated",
            fencing_check: "operator_fencing_evidence_present",
            audit_event: "hadr.node.deregister.requested",
            required_permissions: &["FenceNode", "UpdateClusterManifest"],
            failure_mode: None,
            message: "dry-run accepted: durable runtime membership store would deregister node",
        })?;
        print_node_report(&report, options.json_output);
        return Ok(());
    }
    if !options.apply {
        return Err(cli_error(
            "hadr node deregister with --membership-store requires --apply or --dry-run",
        ));
    }

    let store = open_membership_store(path)?;
    let snapshot = store.deregister_node(node_id)?;
    let report = build_node_report(NodeReportInput {
        contract_preview: false,
        durable_backend: true,
        action: "deregister",
        node_id: Some(options.node_id),
        role: None,
        membership_epoch: Some(snapshot.epoch().get()),
        members: member_reports(Some(&snapshot)),
        dry_run: false,
        would_apply: true,
        quorum_check: "durable_membership_snapshot_updated",
        fencing_check: "operator_fencing_evidence_present",
        audit_event: "hadr.node.deregister.applied",
        required_permissions: &["FenceNode", "UpdateClusterManifest"],
        failure_mode: None,
        message: "durable HADR membership store deregistered node",
    })?;
    print_node_report(&report, options.json_output);
    Ok(())
}

pub(in crate::hadr::node) fn run_durable_node_fence(
    options: &NodeFenceOptions,
    path: &Path,
) -> AndromedaResult<()> {
    if !options.has_fencing_evidence {
        return Err(cli_error(
            "hadr node fence requires --fencing-evidence <evidence-id>",
        ));
    }
    let node_id = HadrNodeId::new(options.node_id);

    if options.dry_run {
        let snapshot = load_membership_snapshot(path)?;
        if !snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.contains(node_id))
        {
            return Err(cli_error("hadr node fence target node is not registered"));
        }
        let report = build_node_report(NodeReportInput {
            contract_preview: false,
            durable_backend: true,
            action: "fence",
            node_id: Some(options.node_id),
            role: None,
            membership_epoch: Some(membership_epoch(snapshot.as_ref())),
            members: member_reports(snapshot.as_ref()),
            dry_run: true,
            would_apply: true,
            quorum_check: "durable_membership_snapshot_validated",
            fencing_check: "operator_fencing_evidence_present",
            audit_event: "hadr.node.fence.requested",
            required_permissions: &["FenceNode"],
            failure_mode: None,
            message: "dry-run accepted: durable runtime membership store would fence node",
        })?;
        print_node_report(&report, options.json_output);
        return Ok(());
    }
    if !options.apply {
        return Err(cli_error(
            "hadr node fence with --membership-store requires --apply or --dry-run",
        ));
    }

    let store = open_membership_store(path)?;
    let snapshot = store.fence_node(node_id)?;
    let report = build_node_report(NodeReportInput {
        contract_preview: false,
        durable_backend: true,
        action: "fence",
        node_id: Some(options.node_id),
        role: snapshot.get(node_id).map(|node| role_label(node.role)),
        membership_epoch: Some(snapshot.epoch().get()),
        members: member_reports(Some(&snapshot)),
        dry_run: false,
        would_apply: true,
        quorum_check: "durable_membership_snapshot_updated",
        fencing_check: "operator_fencing_evidence_present",
        audit_event: "hadr.node.fence.applied",
        required_permissions: &["FenceNode"],
        failure_mode: None,
        message: "durable HADR membership store fenced node",
    })?;
    print_node_report(&report, options.json_output);
    Ok(())
}
