use crate::error::cli_error;
use andromeda_core::AndromedaResult;
use andromeda_storage::{HadrNodeId, HadrQuorumMembership};

use super::output::{print_hadr_node_help, print_node_report};
use super::parsing::{has_dry_run_option, has_json_option, option_value, parse_node_id_arg};
use super::types::NodeManagementReport;

pub(super) fn run_hadr_node(args: &[String]) -> AndromedaResult<()> {
    match args.first().map(String::as_str) {
        Some("register") => run_hadr_node_register(&args[1..]),
        Some("deregister") => run_hadr_node_deregister(&args[1..]),
        Some("list") => run_hadr_node_list(&args[1..]),
        Some("status") => run_hadr_node_status(&args[1..]),
        Some("-h" | "--help" | "help") | None => {
            print_hadr_node_help();
            Ok(())
        }
        Some(cmd) => Err(cli_error(format!(
            "unknown hadr node subcommand `{cmd}`; run `andromeda-cli hadr node --help`"
        ))),
    }
}

pub(super) fn run_hadr_node_register(args: &[String]) -> AndromedaResult<()> {
    let json_output = has_json_option(args);
    let dry_run = has_dry_run_option(args);
    let node_id = parse_node_id_arg(args, "hadr node register requires <node-id>")?;
    let role = option_value(args, "--role").unwrap_or("replica");

    if role != "replica" {
        return Err(cli_error(
            "hadr node register only accepts `--role replica`; primary/candidate registration is reserved for promotion protocol",
        ));
    }
    if !dry_run {
        return Err(cli_error(
            "hadr node register is contract-only in V1.0 scope; rerun with --dry-run",
        ));
    }

    let report = build_node_report(
        "register",
        Some(node_id),
        Some(role),
        dry_run,
        false,
        "passes_static_membership_validation",
        "no_fencing_token_issued",
        "hadr.node.register.requested",
        &["UpdateClusterManifest"],
        None,
        "dry-run accepted: replica node registration would require durable membership update and audit emission",
    )?;
    print_node_report(&report, json_output);
    Ok(())
}

pub(super) fn run_hadr_node_deregister(args: &[String]) -> AndromedaResult<()> {
    let json_output = has_json_option(args);
    let dry_run = has_dry_run_option(args);
    let node_id = parse_node_id_arg(args, "hadr node deregister requires <node-id>")?;
    let has_fencing_evidence = option_value(args, "--fencing-evidence").is_some();

    if !dry_run {
        return Err(cli_error(
            "hadr node deregister is contract-only in V1.0 scope; rerun with --dry-run",
        ));
    }
    if !has_fencing_evidence {
        return Err(cli_error(
            "hadr node deregister dry-run requires --fencing-evidence <evidence-id>",
        ));
    }

    let report = build_node_report(
        "deregister",
        Some(node_id),
        None,
        dry_run,
        false,
        "requires_quorum_after_removal",
        "requires_operator_fencing_evidence",
        "hadr.node.deregister.requested",
        &["FenceNode", "UpdateClusterManifest"],
        None,
        "dry-run accepted: node deregistration would require quorum preservation, fencing evidence, durable membership update, and audit emission",
    )?;
    print_node_report(&report, json_output);
    Ok(())
}

pub(super) fn run_hadr_node_list(args: &[String]) -> AndromedaResult<()> {
    let json_output = has_json_option(args);
    let report = build_node_report(
        "list",
        None,
        None,
        false,
        false,
        "read_only_snapshot",
        "not_applicable",
        "hadr.node.list.requested",
        &["InspectPlans"],
        None,
        "contract scaffold: durable node list backend is not wired; showing command contract only",
    )?;
    print_node_report(&report, json_output);
    Ok(())
}

pub(super) fn run_hadr_node_status(args: &[String]) -> AndromedaResult<()> {
    let json_output = has_json_option(args);
    let node_id = parse_node_id_arg(args, "hadr node status requires <node-id>")?;
    let report = build_node_report(
        "status",
        Some(node_id),
        None,
        false,
        false,
        "read_only_snapshot",
        "not_applicable",
        "hadr.node.status.requested",
        &["InspectPlans"],
        None,
        "contract scaffold: durable node status backend is not wired; showing command contract only",
    )?;
    print_node_report(&report, json_output);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_node_report(
    action: &str,
    node_id: Option<u64>,
    role: Option<&str>,
    dry_run: bool,
    would_apply: bool,
    quorum_check: &str,
    fencing_check: &str,
    audit_event: &str,
    required_permissions: &[&str],
    failure_mode: Option<&str>,
    message: &str,
) -> AndromedaResult<NodeManagementReport> {
    if let Some(node_id) = node_id {
        let membership = HadrQuorumMembership::new(vec![HadrNodeId::new(node_id)])?;
        debug_assert!(membership.contains(HadrNodeId::new(node_id)));
    }

    Ok(NodeManagementReport {
        action: action.to_string(),
        node_id,
        role: role.map(str::to_string),
        dry_run,
        would_apply,
        quorum_check: quorum_check.to_string(),
        fencing_check: fencing_check.to_string(),
        audit_event: audit_event.to_string(),
        required_permissions: required_permissions
            .iter()
            .map(|permission| (*permission).to_string())
            .collect(),
        failure_mode: failure_mode.map(str::to_string),
        message: message.to_string(),
    })
}
