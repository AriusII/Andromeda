use std::path::PathBuf;

use crate::diagnostic_json::JSON_FLAG;
use crate::error::cli_error;
use crate::hadr::parsing::parse_membership_store_option;
use crate::parse::{next_option_value_rejecting_flag, parse_u64};
use andromeda_core::AndromedaResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::hadr::node) struct NodeRegisterOptions {
    pub(in crate::hadr::node) node_id: u64,
    pub(in crate::hadr::node) role: String,
    pub(in crate::hadr::node) dry_run: bool,
    pub(in crate::hadr::node) apply: bool,
    pub(in crate::hadr::node) json_output: bool,
    pub(in crate::hadr::node) membership_store: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::hadr::node) struct NodeDeregisterOptions {
    pub(in crate::hadr::node) node_id: u64,
    pub(in crate::hadr::node) dry_run: bool,
    pub(in crate::hadr::node) apply: bool,
    pub(in crate::hadr::node) has_fencing_evidence: bool,
    pub(in crate::hadr::node) json_output: bool,
    pub(in crate::hadr::node) membership_store: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::hadr::node) struct NodeFenceOptions {
    pub(in crate::hadr::node) node_id: u64,
    pub(in crate::hadr::node) dry_run: bool,
    pub(in crate::hadr::node) apply: bool,
    pub(in crate::hadr::node) has_fencing_evidence: bool,
    pub(in crate::hadr::node) json_output: bool,
    pub(in crate::hadr::node) membership_store: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FencedNodeMutationOptions {
    node_id: u64,
    dry_run: bool,
    apply: bool,
    has_fencing_evidence: bool,
    json_output: bool,
    membership_store: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::hadr::node) struct NodeListOptions {
    pub(in crate::hadr::node) json_output: bool,
    pub(in crate::hadr::node) membership_store: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::hadr::node) struct NodeStatusOptions {
    pub(in crate::hadr::node) node_id: u64,
    pub(in crate::hadr::node) json_output: bool,
    pub(in crate::hadr::node) membership_store: Option<PathBuf>,
}

pub(in crate::hadr::node) fn parse_node_register_options(
    args: &[String],
) -> AndromedaResult<NodeRegisterOptions> {
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

pub(in crate::hadr::node) fn parse_node_deregister_options(
    args: &[String],
) -> AndromedaResult<NodeDeregisterOptions> {
    let options = parse_fenced_node_mutation_options(args, FencedNodeMutationCommand::Deregister)?;
    Ok(NodeDeregisterOptions {
        node_id: options.node_id,
        dry_run: options.dry_run,
        apply: options.apply,
        has_fencing_evidence: options.has_fencing_evidence,
        json_output: options.json_output,
        membership_store: options.membership_store,
    })
}

pub(in crate::hadr::node) fn parse_node_fence_options(
    args: &[String],
) -> AndromedaResult<NodeFenceOptions> {
    let options = parse_fenced_node_mutation_options(args, FencedNodeMutationCommand::Fence)?;
    Ok(NodeFenceOptions {
        node_id: options.node_id,
        dry_run: options.dry_run,
        apply: options.apply,
        has_fencing_evidence: options.has_fencing_evidence,
        json_output: options.json_output,
        membership_store: options.membership_store,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FencedNodeMutationCommand {
    Deregister,
    Fence,
}

impl FencedNodeMutationCommand {
    fn name(self) -> &'static str {
        match self {
            Self::Deregister => "deregister",
            Self::Fence => "fence",
        }
    }

    fn unknown_option_message(self) -> String {
        format!(
            "unknown hadr node {} option; supported options are --fencing-evidence, --apply, --dry-run, --membership-store, --state-dir, and --json",
            self.name()
        )
    }

    fn duplicate_node_message(self) -> String {
        format!("hadr node {} accepts exactly one <node-id>", self.name())
    }

    fn missing_node_message(self) -> String {
        format!("hadr node {} requires <node-id>", self.name())
    }
}

fn parse_fenced_node_mutation_options(
    args: &[String],
    command: FencedNodeMutationCommand,
) -> AndromedaResult<FencedNodeMutationOptions> {
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
                return Err(cli_error(command.unknown_option_message()));
            }
            value => {
                if node_id.is_some() {
                    return Err(cli_error(command.duplicate_node_message()));
                }
                node_id = Some(parse_nonzero_node_id(value)?);
            }
        }
        index += 1;
    }

    let Some(node_id) = node_id else {
        return Err(cli_error(command.missing_node_message()));
    };

    Ok(FencedNodeMutationOptions {
        node_id,
        dry_run,
        apply,
        has_fencing_evidence,
        json_output,
        membership_store,
    })
}

pub(in crate::hadr::node) fn parse_node_list_options(
    args: &[String],
) -> AndromedaResult<NodeListOptions> {
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

pub(in crate::hadr::node) fn parse_node_status_options(
    args: &[String],
) -> AndromedaResult<NodeStatusOptions> {
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
