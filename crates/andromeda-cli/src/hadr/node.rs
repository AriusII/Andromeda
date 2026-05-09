mod durable;
mod options;
mod report;

use crate::error::cli_error;
use andromeda_error::AndromedaResult;
use andromeda_hadr::HadrNodeId;

use super::output::{print_hadr_node_help, print_node_report};
use super::runtime::{load_membership_snapshot, member_reports, membership_epoch, role_label};
use super::types::NodeMembershipMemberReport;
use durable::{run_durable_node_deregister, run_durable_node_fence, run_durable_node_register};
use options::{
    parse_node_deregister_options, parse_node_fence_options, parse_node_list_options,
    parse_node_register_options, parse_node_status_options,
};
use report::{NodeReportInput, build_node_report};

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
        },
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
                vec![NodeMembershipMemberReport {
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
