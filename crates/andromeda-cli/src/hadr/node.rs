use std::path::{Path, PathBuf};

use crate::diagnostic_json::JSON_FLAG;
use crate::error::cli_error;
use crate::parse::{next_option_value_rejecting_flag, parse_u64};
use andromeda_core::AndromedaResult;
use andromeda_storage::{HadrMembershipStore, HadrNodeId, HadrNodeRole, HadrQuorumMembership};

use super::output::{print_hadr_node_help, print_node_report};
use super::parsing::parse_membership_store_option;
use super::runtime::{
    load_membership_snapshot, member_reports, membership_epoch, open_membership_store, role_label,
};
use super::types::NodeManagementReport;

pub(super) fn run_hadr_node(args: &[String]) -> AndromedaResult<()> {
    match args.first().map(String::as_str) {
        Some("register") => run_hadr_node_register(&args[1..]),
        Some("deregister") => run_hadr_node_deregister(&args[1..]),
        Some("fence") => run_hadr_node_fence(&args[1..]),
        Some("list") => run_hadr_node_list(&args[1..]),
        Some("status") => run_hadr_node_status(&args[1..]),
        Some("-h" | "--help" | "help") | None => {
            print_hadr_node_help();
            Ok(())
        }
        Some(_) => Err(cli_error(
            "unknown hadr node subcommand; run `andromeda-cli hadr node --help`",
        )),
    }
}

pub(super) fn run_hadr_node_register(args: &[String]) -> AndromedaResult<()> {
    let options = parse_node_register_options(args)?;

    if options.role != "replica" {
        return Err(cli_error(
            "hadr node register only accepts `--role replica`; primary/candidate registration is reserved for promotion protocol",
        ));
    }
    if options.dry_run && options.apply {
        return Err(cli_error(
            "hadr node register accepts only one of --apply or --dry-run",
        ));
    }
    if let Some(path) = options.membership_store.as_deref() {
        return run_durable_node_register(&options, path);
    }
    if options.apply {
        return Err(cli_error(
            "hadr node register --apply requires --membership-store <file>",
        ));
    }
    if !options.dry_run {
        return Err(cli_error(
            "hadr node register is contract-only in V1.0 scope; rerun with --dry-run",
        ));
    }

    let report = build_node_report(NodeReportInput {
        contract_preview: true,
        durable_backend: false,
        action: "register",
        node_id: Some(options.node_id),
        role: Some(&options.role),
        membership_epoch: None,
        members: Vec::new(),
        dry_run: options.dry_run,
        would_apply: false,
        quorum_check: "passes_static_membership_validation",
        fencing_check: "no_fencing_token_issued",
        audit_event: "hadr.node.register.requested",
        required_permissions: &["UpdateClusterManifest"],
        failure_mode: None,
        message: "dry-run accepted: replica node registration would require durable membership update and audit emission",
    })?;
    print_node_report(&report, options.json_output);
    Ok(())
}

pub(super) fn run_hadr_node_deregister(args: &[String]) -> AndromedaResult<()> {
    let options = parse_node_deregister_options(args)?;

    if options.dry_run && options.apply {
        return Err(cli_error(
            "hadr node deregister accepts only one of --apply or --dry-run",
        ));
    }
    if let Some(path) = options.membership_store.as_deref() {
        return run_durable_node_deregister(&options, path);
    }
    if options.apply {
        return Err(cli_error(
            "hadr node deregister --apply requires --membership-store <file>",
        ));
    }
    if !options.dry_run {
        return Err(cli_error(
            "hadr node deregister is contract-only in V1.0 scope; rerun with --dry-run",
        ));
    }
    if !options.has_fencing_evidence {
        return Err(cli_error(
            "hadr node deregister dry-run requires --fencing-evidence <evidence-id>",
        ));
    }

    let report = build_node_report(NodeReportInput {
        contract_preview: true,
        durable_backend: false,
        action: "deregister",
        node_id: Some(options.node_id),
        role: None,
        membership_epoch: None,
        members: Vec::new(),
        dry_run: options.dry_run,
        would_apply: false,
        quorum_check: "requires_quorum_after_removal",
        fencing_check: "requires_operator_fencing_evidence",
        audit_event: "hadr.node.deregister.requested",
        required_permissions: &["FenceNode", "UpdateClusterManifest"],
        failure_mode: None,
        message: "dry-run accepted: node deregistration would require quorum preservation, fencing evidence, durable membership update, and audit emission",
    })?;
    print_node_report(&report, options.json_output);
    Ok(())
}

pub(super) fn run_hadr_node_fence(args: &[String]) -> AndromedaResult<()> {
    let options = parse_node_fence_options(args)?;

    if options.dry_run && options.apply {
        return Err(cli_error(
            "hadr node fence accepts only one of --apply or --dry-run",
        ));
    }
    if let Some(path) = options.membership_store.as_deref() {
        return run_durable_node_fence(&options, path);
    }
    if options.apply {
        return Err(cli_error(
            "hadr node fence --apply requires --membership-store <file>",
        ));
    }
    if !options.dry_run {
        return Err(cli_error(
            "hadr node fence is contract-only in V1.0 scope; rerun with --dry-run",
        ));
    }
    if !options.has_fencing_evidence {
        return Err(cli_error(
            "hadr node fence dry-run requires --fencing-evidence <evidence-id>",
        ));
    }

    let report = build_node_report(NodeReportInput {
        contract_preview: true,
        durable_backend: false,
        action: "fence",
        node_id: Some(options.node_id),
        role: None,
        membership_epoch: None,
        members: Vec::new(),
        dry_run: options.dry_run,
        would_apply: false,
        quorum_check: "requires_quorum_after_fencing",
        fencing_check: "requires_operator_fencing_evidence",
        audit_event: "hadr.node.fence.requested",
        required_permissions: &["FenceNode"],
        failure_mode: None,
        message: "dry-run accepted: node fencing would require fencing evidence, durable membership update, and audit emission",
    })?;
    print_node_report(&report, options.json_output);
    Ok(())
}

pub(super) fn run_hadr_node_list(args: &[String]) -> AndromedaResult<()> {
    let options = parse_node_list_options(args)?;
    if let Some(path) = options.membership_store.as_deref() {
        let snapshot = load_membership_snapshot(path)?;
        let report = build_node_report(NodeReportInput {
            contract_preview: false,
            durable_backend: true,
            action: "list",
            node_id: None,
            role: None,
            membership_epoch: Some(membership_epoch(snapshot.as_ref())),
            members: member_reports(snapshot.as_ref()),
            dry_run: false,
            would_apply: false,
            quorum_check: "durable_membership_snapshot",
            fencing_check: "not_applicable",
            audit_event: "hadr.node.list.requested",
            required_permissions: &["InspectPlans"],
            failure_mode: None,
            message: "durable HADR membership store loaded",
        })?;
        print_node_report(&report, options.json_output);
        return Ok(());
    }
    let report = build_node_report(NodeReportInput {
        contract_preview: true,
        durable_backend: false,
        action: "list",
        node_id: None,
        role: None,
        membership_epoch: None,
        members: Vec::new(),
        dry_run: false,
        would_apply: false,
        quorum_check: "read_only_snapshot",
        fencing_check: "not_applicable",
        audit_event: "hadr.node.list.requested",
        required_permissions: &["InspectPlans"],
        failure_mode: None,
        message: "contract preview: durable node list backend is not wired; showing command contract only",
    })?;
    print_node_report(&report, options.json_output);
    Ok(())
}

pub(super) fn run_hadr_node_status(args: &[String]) -> AndromedaResult<()> {
    let options = parse_node_status_options(args)?;
    if let Some(path) = options.membership_store.as_deref() {
        let snapshot = load_membership_snapshot(path)?;
        let node = snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.get(HadrNodeId::new(options.node_id)));
        let members = node
            .map(|node| {
                vec![super::types::NodeMembershipMemberReport {
                    node_id: node.id.get(),
                    role: role_label(node.role).to_string(),
                    role_epoch: node.role_epoch.get(),
                }]
            })
            .unwrap_or_default();
        let report = build_node_report(NodeReportInput {
            contract_preview: false,
            durable_backend: true,
            action: "status",
            node_id: Some(options.node_id),
            role: node.map(|node| role_label(node.role)),
            membership_epoch: Some(membership_epoch(snapshot.as_ref())),
            members,
            dry_run: false,
            would_apply: false,
            quorum_check: "durable_membership_snapshot",
            fencing_check: "not_applicable",
            audit_event: "hadr.node.status.requested",
            required_permissions: &["InspectPlans"],
            failure_mode: node.is_none().then_some("node_not_registered"),
            message: if node.is_some() {
                "durable HADR membership node loaded"
            } else {
                "durable HADR membership store loaded; node id is not registered"
            },
        })?;
        print_node_report(&report, options.json_output);
        return Ok(());
    }
    let report = build_node_report(NodeReportInput {
        contract_preview: true,
        durable_backend: false,
        action: "status",
        node_id: Some(options.node_id),
        role: None,
        membership_epoch: None,
        members: Vec::new(),
        dry_run: false,
        would_apply: false,
        quorum_check: "read_only_snapshot",
        fencing_check: "not_applicable",
        audit_event: "hadr.node.status.requested",
        required_permissions: &["InspectPlans"],
        failure_mode: None,
        message: "contract preview: durable node status backend is not wired; showing command contract only",
    })?;
    print_node_report(&report, options.json_output);
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NodeRegisterOptions {
    node_id: u64,
    role: String,
    dry_run: bool,
    apply: bool,
    json_output: bool,
    membership_store: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NodeDeregisterOptions {
    node_id: u64,
    dry_run: bool,
    apply: bool,
    has_fencing_evidence: bool,
    json_output: bool,
    membership_store: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NodeFenceOptions {
    node_id: u64,
    dry_run: bool,
    apply: bool,
    has_fencing_evidence: bool,
    json_output: bool,
    membership_store: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NodeListOptions {
    json_output: bool,
    membership_store: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NodeStatusOptions {
    node_id: u64,
    json_output: bool,
    membership_store: Option<PathBuf>,
}

struct NodeReportInput<'a> {
    contract_preview: bool,
    durable_backend: bool,
    action: &'a str,
    node_id: Option<u64>,
    role: Option<&'a str>,
    membership_epoch: Option<u64>,
    members: Vec<super::types::NodeMembershipMemberReport>,
    dry_run: bool,
    would_apply: bool,
    quorum_check: &'a str,
    fencing_check: &'a str,
    audit_event: &'a str,
    required_permissions: &'a [&'a str],
    failure_mode: Option<&'a str>,
    message: &'a str,
}

fn parse_node_register_options(args: &[String]) -> AndromedaResult<NodeRegisterOptions> {
    let mut node_id = None;
    let mut role = "replica".to_string();
    let mut dry_run = false;
    let mut apply = false;
    let mut json_output = false;
    let mut membership_store = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            JSON_FLAG => json_output = true,
            "--dry-run" => dry_run = true,
            "--apply" => apply = true,
            "--membership-store" | "--state-dir" => {
                parse_membership_store_option(args, &mut index, &mut membership_store)?;
            }
            "--role" => {
                role =
                    next_option_value_rejecting_flag(args, &mut index, "--role requires a value")?
                        .to_string();
            }
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown hadr node register option; supported options are --role, --apply, --dry-run, --membership-store, --state-dir, and --json",
                ));
            }
            value => {
                if node_id.is_some() {
                    return Err(cli_error(
                        "hadr node register accepts exactly one <node-id>",
                    ));
                }
                node_id = Some(parse_nonzero_node_id(value)?);
            }
        }
        index += 1;
    }

    let Some(node_id) = node_id else {
        return Err(cli_error("hadr node register requires <node-id>"));
    };

    Ok(NodeRegisterOptions {
        node_id,
        role,
        dry_run,
        apply,
        json_output,
        membership_store,
    })
}

fn parse_node_deregister_options(args: &[String]) -> AndromedaResult<NodeDeregisterOptions> {
    let mut node_id = None;
    let mut dry_run = false;
    let mut apply = false;
    let mut has_fencing_evidence = false;
    let mut json_output = false;
    let mut membership_store = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            JSON_FLAG => json_output = true,
            "--dry-run" => dry_run = true,
            "--apply" => apply = true,
            "--membership-store" | "--state-dir" => {
                parse_membership_store_option(args, &mut index, &mut membership_store)?;
            }
            "--fencing-evidence" => {
                next_option_value_rejecting_flag(
                    args,
                    &mut index,
                    "--fencing-evidence requires a value",
                )?;
                has_fencing_evidence = true;
            }
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown hadr node deregister option; supported options are --fencing-evidence, --apply, --dry-run, --membership-store, --state-dir, and --json",
                ));
            }
            value => {
                if node_id.is_some() {
                    return Err(cli_error(
                        "hadr node deregister accepts exactly one <node-id>",
                    ));
                }
                node_id = Some(parse_nonzero_node_id(value)?);
            }
        }
        index += 1;
    }

    let Some(node_id) = node_id else {
        return Err(cli_error("hadr node deregister requires <node-id>"));
    };

    Ok(NodeDeregisterOptions {
        node_id,
        dry_run,
        apply,
        has_fencing_evidence,
        json_output,
        membership_store,
    })
}

fn parse_node_fence_options(args: &[String]) -> AndromedaResult<NodeFenceOptions> {
    let mut node_id = None;
    let mut dry_run = false;
    let mut apply = false;
    let mut has_fencing_evidence = false;
    let mut json_output = false;
    let mut membership_store = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            JSON_FLAG => json_output = true,
            "--dry-run" => dry_run = true,
            "--apply" => apply = true,
            "--membership-store" | "--state-dir" => {
                parse_membership_store_option(args, &mut index, &mut membership_store)?;
            }
            "--fencing-evidence" => {
                next_option_value_rejecting_flag(
                    args,
                    &mut index,
                    "--fencing-evidence requires a value",
                )?;
                has_fencing_evidence = true;
            }
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown hadr node fence option; supported options are --fencing-evidence, --apply, --dry-run, --membership-store, --state-dir, and --json",
                ));
            }
            value => {
                if node_id.is_some() {
                    return Err(cli_error("hadr node fence accepts exactly one <node-id>"));
                }
                node_id = Some(parse_nonzero_node_id(value)?);
            }
        }
        index += 1;
    }

    let Some(node_id) = node_id else {
        return Err(cli_error("hadr node fence requires <node-id>"));
    };

    Ok(NodeFenceOptions {
        node_id,
        dry_run,
        apply,
        has_fencing_evidence,
        json_output,
        membership_store,
    })
}

fn parse_node_list_options(args: &[String]) -> AndromedaResult<NodeListOptions> {
    let mut json_output = false;
    let mut membership_store = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            JSON_FLAG => json_output = true,
            "--membership-store" | "--state-dir" => {
                parse_membership_store_option(args, &mut index, &mut membership_store)?;
            }
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown hadr node list option; supported options are --membership-store, --state-dir, and --json",
                ));
            }
            _ => {
                return Err(cli_error(
                    "unexpected hadr node list argument; supported options are --membership-store, --state-dir, and --json",
                ));
            }
        }
        index += 1;
    }

    Ok(NodeListOptions {
        json_output,
        membership_store,
    })
}

fn parse_node_status_options(args: &[String]) -> AndromedaResult<NodeStatusOptions> {
    let mut node_id = None;
    let mut json_output = false;
    let mut membership_store = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            JSON_FLAG => json_output = true,
            "--membership-store" | "--state-dir" => {
                parse_membership_store_option(args, &mut index, &mut membership_store)?;
            }
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown hadr node status option; supported options are --membership-store, --state-dir, and --json",
                ));
            }
            value => {
                if node_id.is_some() {
                    return Err(cli_error("hadr node status accepts exactly one <node-id>"));
                }
                node_id = Some(parse_nonzero_node_id(value)?);
            }
        }
        index += 1;
    }

    let Some(node_id) = node_id else {
        return Err(cli_error("hadr node status requires <node-id>"));
    };

    Ok(NodeStatusOptions {
        node_id,
        json_output,
        membership_store,
    })
}

fn parse_nonzero_node_id(value: &str) -> AndromedaResult<u64> {
    let node_id = parse_u64(value, "node-id must be an unsigned integer")?;
    if node_id == 0 {
        return Err(cli_error("node-id must not be zero"));
    }
    Ok(node_id)
}

fn build_node_report(input: NodeReportInput<'_>) -> AndromedaResult<NodeManagementReport> {
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

fn run_durable_node_register(options: &NodeRegisterOptions, path: &Path) -> AndromedaResult<()> {
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

fn run_durable_node_deregister(
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

fn run_durable_node_fence(options: &NodeFenceOptions, path: &Path) -> AndromedaResult<()> {
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
