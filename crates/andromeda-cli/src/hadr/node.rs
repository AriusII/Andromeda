use crate::diagnostic_json::{JSON_FLAG, parse_json_flag};
use crate::error::cli_error;
use crate::parse::{next_option_value_rejecting_flag, parse_u64};
use andromeda_core::AndromedaResult;
use andromeda_storage::{HadrNodeId, HadrQuorumMembership};

use super::output::{print_hadr_node_help, print_node_report};
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
    let options = parse_node_register_options(args)?;

    if options.role != "replica" {
        return Err(cli_error(
            "hadr node register only accepts `--role replica`; primary/candidate registration is reserved for promotion protocol",
        ));
    }
    if !options.dry_run {
        return Err(cli_error(
            "hadr node register is contract-only in V1.0 scope; rerun with --dry-run",
        ));
    }

    let report = build_node_report(NodeReportInput {
        action: "register",
        node_id: Some(options.node_id),
        role: Some(&options.role),
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
        action: "deregister",
        node_id: Some(options.node_id),
        role: None,
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

pub(super) fn run_hadr_node_list(args: &[String]) -> AndromedaResult<()> {
    let json_output = parse_json_flag(args, "hadr node list")?;
    let report = build_node_report(NodeReportInput {
        action: "list",
        node_id: None,
        role: None,
        dry_run: false,
        would_apply: false,
        quorum_check: "read_only_snapshot",
        fencing_check: "not_applicable",
        audit_event: "hadr.node.list.requested",
        required_permissions: &["InspectPlans"],
        failure_mode: None,
        message: "contract preview: durable node list backend is not wired; showing command contract only",
    })?;
    print_node_report(&report, json_output);
    Ok(())
}

pub(super) fn run_hadr_node_status(args: &[String]) -> AndromedaResult<()> {
    let options = parse_node_status_options(args)?;
    let report = build_node_report(NodeReportInput {
        action: "status",
        node_id: Some(options.node_id),
        role: None,
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
    json_output: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NodeDeregisterOptions {
    node_id: u64,
    dry_run: bool,
    has_fencing_evidence: bool,
    json_output: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NodeStatusOptions {
    node_id: u64,
    json_output: bool,
}

struct NodeReportInput<'a> {
    action: &'a str,
    node_id: Option<u64>,
    role: Option<&'a str>,
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
    let mut json_output = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            JSON_FLAG => json_output = true,
            "--dry-run" => dry_run = true,
            "--role" => {
                role =
                    next_option_value_rejecting_flag(args, &mut index, "--role requires a value")?
                        .to_string();
            }
            opt if opt.starts_with("--") => {
                return Err(cli_error(format!(
                    "unknown hadr node register option: {opt}; supported options are --role, --dry-run, and --json"
                )));
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
        json_output,
    })
}

fn parse_node_deregister_options(args: &[String]) -> AndromedaResult<NodeDeregisterOptions> {
    let mut node_id = None;
    let mut dry_run = false;
    let mut has_fencing_evidence = false;
    let mut json_output = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            JSON_FLAG => json_output = true,
            "--dry-run" => dry_run = true,
            "--fencing-evidence" => {
                next_option_value_rejecting_flag(
                    args,
                    &mut index,
                    "--fencing-evidence requires a value",
                )?;
                has_fencing_evidence = true;
            }
            opt if opt.starts_with("--") => {
                return Err(cli_error(format!(
                    "unknown hadr node deregister option: {opt}; supported options are --fencing-evidence, --dry-run, and --json"
                )));
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
        has_fencing_evidence,
        json_output,
    })
}

fn parse_node_status_options(args: &[String]) -> AndromedaResult<NodeStatusOptions> {
    let mut node_id = None;
    let mut json_output = false;

    for arg in args {
        match arg.as_str() {
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(format!(
                    "unknown hadr node status option: {opt}; supported option is --json"
                )));
            }
            value => {
                if node_id.is_some() {
                    return Err(cli_error("hadr node status accepts exactly one <node-id>"));
                }
                node_id = Some(parse_nonzero_node_id(value)?);
            }
        }
    }

    let Some(node_id) = node_id else {
        return Err(cli_error("hadr node status requires <node-id>"));
    };

    Ok(NodeStatusOptions {
        node_id,
        json_output,
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
        action: input.action.to_string(),
        node_id: input.node_id,
        role: input.role.map(str::to_string),
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
