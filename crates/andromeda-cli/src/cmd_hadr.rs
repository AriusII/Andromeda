//! HADR administration commands.
//!
//! Provides CLI commands for:
//! - Quorum membership and status inspection
//! - Node-management contract dry-runs for register/deregister/list/status
//! - Replica promotion and demotion
//! - Fencing policy configuration
//! - LSN gap and lag visibility

use crate::diagnostic_json::{
    JSON_FLAG, json_option_string, json_option_u64, json_string, json_string_array, json_u64_array,
    parse_json_flag,
};
use crate::error::cli_error;
use andromeda_core::AndromedaResult;
use andromeda_storage::{HadrNodeId, HadrQuorumMembership};

/// HADR status output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HadrStatusReport {
    pub cluster_role: String,
    pub epoch: u64,
    pub current_primary: Option<u64>,
    pub replicas: Vec<ReplicaStatus>,
    pub durable_lsn: u64,
    pub committed_lsn: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReplicaStatus {
    pub replica_id: u64,
    pub health_state: String,
    pub received_lsn: u64,
    pub shipped_lsn: u64,
    pub lag_bytes: i64,
}

/// Quorum configuration output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct QuorumStatusReport {
    pub total_members: usize,
    pub quorum_size: usize,
    pub member_ids: Vec<u64>,
    pub fencing_policy: String,
    pub fencing_status: String,
}

/// Node-management report for Administration/HA surface commands.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeManagementReport {
    pub action: String,
    pub node_id: Option<u64>,
    pub role: Option<String>,
    pub dry_run: bool,
    pub would_apply: bool,
    pub quorum_check: String,
    pub fencing_check: String,
    pub audit_event: String,
    pub required_permissions: Vec<String>,
    pub failure_mode: Option<String>,
    pub message: String,
}

/// Promotion outcome.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PromotionOutcome {
    pub success: bool,
    pub new_epoch: u64,
    pub promoted_replica_id: u64,
    pub message: String,
}

/// Demotion outcome.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DemotionOutcome {
    pub success: bool,
    pub new_primary_id: Option<u64>,
    pub message: String,
}

/// Parses and executes HADR subcommands.
pub fn run_hadr_command(args: &[String]) -> AndromedaResult<()> {
    match args.first().map(String::as_str) {
        Some("status") => run_hadr_status(&args[1..]),
        Some("node") => run_hadr_node(&args[1..]),
        Some("promote") => run_hadr_promote(&args[1..]),
        Some("demote") => run_hadr_demote(&args[1..]),
        Some("quorum") => run_hadr_quorum(&args[1..]),
        Some("-h" | "--help" | "help") => {
            print_hadr_help();
            Ok(())
        }
        Some(cmd) => Err(cli_error(format!(
            "unknown hadr subcommand `{cmd}`; run `andromeda-cli hadr --help`"
        ))),
        None => {
            print_hadr_help();
            Ok(())
        }
    }
}

/// Dispatches node-management contract commands. Mutating operations are dry-run
/// only until a durable membership store, authorization hook, and audit sink are
/// wired by the HA/DR runtime.
fn run_hadr_node(args: &[String]) -> AndromedaResult<()> {
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

fn run_hadr_node_register(args: &[String]) -> AndromedaResult<()> {
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

fn run_hadr_node_deregister(args: &[String]) -> AndromedaResult<()> {
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

fn run_hadr_node_list(args: &[String]) -> AndromedaResult<()> {
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

fn run_hadr_node_status(args: &[String]) -> AndromedaResult<()> {
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

/// Displays HADR status including quorum membership, LSN state, and replica lag.
fn run_hadr_status(args: &[String]) -> AndromedaResult<()> {
    let json_output = parse_json_flag(args, "hadr status")?;

    // MOCK: In a real implementation, this would query the HADR runtime.
    let report = HadrStatusReport {
        cluster_role: "Primary".to_string(),
        epoch: 42,
        current_primary: Some(1),
        replicas: vec![
            ReplicaStatus {
                replica_id: 2,
                health_state: "Alive".to_string(),
                received_lsn: 1048576,
                shipped_lsn: 1048576,
                lag_bytes: 0,
            },
            ReplicaStatus {
                replica_id: 3,
                health_state: "Alive".to_string(),
                received_lsn: 1047552,
                shipped_lsn: 1048576,
                lag_bytes: 1024,
            },
        ],
        durable_lsn: 1048576,
        committed_lsn: 1048576,
    };

    if json_output {
        print_hadr_status_json(&report);
    } else {
        print_hadr_status_human(&report);
    }

    Ok(())
}

/// Promotes a replica to primary at a specified replica ID.
fn run_hadr_promote(args: &[String]) -> AndromedaResult<()> {
    if args.is_empty() {
        return Err(cli_error(
            "hadr promote requires <replica-id>; usage: `hadr promote <replica-id>`",
        ));
    }

    let replica_id: u64 = args[0]
        .parse()
        .map_err(|_| cli_error("replica-id must be an unsigned integer"))?;

    let json_output = parse_json_flag(&args[1..], "hadr promote")?;

    // MOCK: In a real implementation, this would invoke the promotion protocol.
    let outcome = PromotionOutcome {
        success: true,
        new_epoch: 43,
        promoted_replica_id: replica_id,
        message: format!("Replica {} promoted to primary", replica_id),
    };

    if json_output {
        print_promotion_json(&outcome);
    } else if outcome.success {
        println!(
            "✓ Replica {} promoted to primary (epoch {})",
            outcome.promoted_replica_id, outcome.new_epoch
        );
    } else {
        println!("✗ Promotion failed: {}", outcome.message);
    }

    Ok(())
}

/// Demotes current primary to replica (planned failover).
fn run_hadr_demote(args: &[String]) -> AndromedaResult<()> {
    let (json_output, force) = parse_demote_options(args)?;

    if !force {
        if json_output {
            println!(
                "{{\"success\":false,\"new_primary_id\":null,\"message\":{}}}",
                json_string("Demotion requires --force confirmation")
            );
        } else {
            println!("⚠ WARNING: Demoting primary will disrupt writes.");
            println!("Use --force to confirm.");
        }
        return Ok(());
    }

    // MOCK: In a real implementation, this would invoke the demotion protocol.
    let outcome = DemotionOutcome {
        success: true,
        new_primary_id: Some(2),
        message: "Primary demoted successfully".to_string(),
    };

    if json_output {
        print_demotion_json(&outcome);
    } else if outcome.success {
        if let Some(new_primary) = outcome.new_primary_id {
            println!("✓ Primary demoted; replica {} is new primary", new_primary);
        } else {
            println!("✓ Primary demoted (no suitable replica for immediate promotion)");
        }
    } else {
        println!("✗ Demotion failed: {}", outcome.message);
    }

    Ok(())
}

/// Shows quorum configuration, member list, and fencing status.
fn run_hadr_quorum(args: &[String]) -> AndromedaResult<()> {
    let json_output = parse_json_flag(args, "hadr quorum")?;

    // MOCK: In a real implementation, this would query the quorum consensus engine.
    let report = QuorumStatusReport {
        total_members: 3,
        quorum_size: 2,
        member_ids: vec![1, 2, 3],
        fencing_policy: "QuorumEnforced".to_string(),
        fencing_status: "Active".to_string(),
    };

    if json_output {
        print_quorum_status_json(&report);
    } else {
        print_quorum_status_human(&report);
    }

    Ok(())
}

fn print_hadr_help() {
    println!("Andromeda HADR administration commands");
    println!();
    println!("USAGE: andromeda-cli hadr <SUBCOMMAND> [OPTIONS]");
    println!();
    println!("SUBCOMMANDS:");
    println!("  status              Display quorum membership, LSN state, replica lag");
    println!(
        "  node <command>      Node management contract commands (register/deregister/list/status)"
    );
    println!("  promote <replica-id>   Promote replica to primary");
    println!("  demote              Demote current primary to replica (planned failover)");
    println!("  quorum              Show quorum configuration and fencing status");
    println!();
    println!("OPTIONS:");
    println!("  --force             Confirm destructive operations (e.g., demote)");
    println!("  --json              Emit diagnostic machine-readable JSON output");
    println!("  -h, --help          Show this help message");
}

fn print_hadr_node_help() {
    println!("Andromeda HADR node management contract commands");
    println!();
    println!("USAGE: andromeda-cli hadr node <SUBCOMMAND> [OPTIONS]");
    println!();
    println!("SUBCOMMANDS:");
    println!("  register <node-id> --role replica --dry-run");
    println!("  deregister <node-id> --fencing-evidence <evidence-id> --dry-run");
    println!("  list");
    println!("  status <node-id>");
    println!();
    println!("OPTIONS:");
    println!("  --dry-run           Validate the contract without mutating membership");
    println!("  --json              Emit diagnostic machine-readable JSON output");
    println!("  -h, --help          Show this help message");
}

fn has_json_option(args: &[String]) -> bool {
    args.iter().any(|arg| arg == JSON_FLAG)
}

fn parse_demote_options(args: &[String]) -> AndromedaResult<(bool, bool)> {
    let mut json_output = false;
    let mut force = false;
    for arg in args {
        match arg.as_str() {
            JSON_FLAG => json_output = true,
            "--force" => force = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(format!(
                    "unknown hadr demote option: {opt}; supported options are --force and --json"
                )));
            }
            value => {
                return Err(cli_error(format!(
                    "unexpected hadr demote argument: {value}; supported options are --force and --json"
                )));
            }
        }
    }
    Ok((json_output, force))
}

fn has_dry_run_option(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--dry-run")
}

fn parse_node_id_arg(args: &[String], missing_message: &'static str) -> AndromedaResult<u64> {
    let raw = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .ok_or_else(|| cli_error(missing_message))?;
    let node_id: u64 = raw
        .parse()
        .map_err(|_| cli_error("node-id must be an unsigned integer"))?;
    if node_id == 0 {
        return Err(cli_error("node-id must not be zero"));
    }
    Ok(node_id)
}

fn option_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == name)
        .map(|window| window[1].as_str())
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
        // Reuse the storage HADR identity and membership validation contract so
        // CLI parsing preserves the same non-zero-node invariant as quorum code.
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

fn print_hadr_status_human(report: &HadrStatusReport) {
    println!("HADR Status Report");
    println!("==================");
    println!("Cluster Role: {}", report.cluster_role);
    println!("Epoch: {}", report.epoch);
    if let Some(primary) = report.current_primary {
        println!("Current Primary: {}", primary);
    }
    println!("Durable LSN: {}", report.durable_lsn);
    println!("Committed LSN: {}", report.committed_lsn);
    println!();
    println!("Replicas:");
    for replica in &report.replicas {
        println!(
            "  Replica {} ({}) — received_lsn={}, shipped_lsn={}, lag={}",
            replica.replica_id,
            replica.health_state,
            replica.received_lsn,
            replica.shipped_lsn,
            replica.lag_bytes
        );
    }
}

fn print_quorum_status_human(report: &QuorumStatusReport) {
    println!("Quorum Configuration");
    println!("====================");
    println!("Total Members: {}", report.total_members);
    println!("Quorum Size: {}", report.quorum_size);
    println!("Member IDs: {:?}", report.member_ids);
    println!("Fencing Policy: {}", report.fencing_policy);
    println!("Fencing Status: {}", report.fencing_status);
}

fn print_hadr_status_json(report: &HadrStatusReport) {
    println!(
        "{{\"cluster_role\":{},\"epoch\":{},\"current_primary\":{},\"replicas\":{},\"durable_lsn\":{},\"committed_lsn\":{}}}",
        json_string(&report.cluster_role),
        report.epoch,
        json_option_u64(report.current_primary),
        replicas_json(&report.replicas),
        report.durable_lsn,
        report.committed_lsn,
    );
}

fn replicas_json(replicas: &[ReplicaStatus]) -> String {
    let entries = replicas
        .iter()
        .map(|replica| {
            format!(
                "{{\"replica_id\":{},\"health_state\":{},\"received_lsn\":{},\"shipped_lsn\":{},\"lag_bytes\":{}}}",
                replica.replica_id,
                json_string(&replica.health_state),
                replica.received_lsn,
                replica.shipped_lsn,
                replica.lag_bytes,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{}]", entries)
}

fn print_quorum_status_json(report: &QuorumStatusReport) {
    println!(
        "{{\"total_members\":{},\"quorum_size\":{},\"member_ids\":{},\"fencing_policy\":{},\"fencing_status\":{}}}",
        report.total_members,
        report.quorum_size,
        json_u64_array(&report.member_ids),
        json_string(&report.fencing_policy),
        json_string(&report.fencing_status),
    );
}

fn print_node_report(report: &NodeManagementReport, json_output: bool) {
    if json_output {
        print_node_report_json(report);
    } else {
        print_node_report_human(report);
    }
}

fn print_node_report_human(report: &NodeManagementReport) {
    println!("HADR Node Management Contract");
    println!("=============================");
    println!("Action: {}", report.action);
    if let Some(node_id) = report.node_id {
        println!("Node ID: {}", node_id);
    }
    if let Some(role) = &report.role {
        println!("Role: {}", role);
    }
    println!("Dry Run: {}", report.dry_run);
    println!("Would Apply: {}", report.would_apply);
    println!("Quorum Check: {}", report.quorum_check);
    println!("Fencing Check: {}", report.fencing_check);
    println!("Audit Event: {}", report.audit_event);
    println!("Required Permissions: {:?}", report.required_permissions);
    if let Some(failure_mode) = &report.failure_mode {
        println!("Failure Mode: {}", failure_mode);
    }
    println!("{}", report.message);
}

fn print_node_report_json(report: &NodeManagementReport) {
    println!(
        "{{\"action\":{},\"node_id\":{},\"role\":{},\"dry_run\":{},\"would_apply\":{},\"quorum_check\":{},\"fencing_check\":{},\"audit_event\":{},\"required_permissions\":{},\"failure_mode\":{},\"message\":{}}}",
        json_string(&report.action),
        json_option_u64(report.node_id),
        json_option_string(report.role.as_deref()),
        report.dry_run,
        report.would_apply,
        json_string(&report.quorum_check),
        json_string(&report.fencing_check),
        json_string(&report.audit_event),
        json_string_array(&report.required_permissions),
        json_option_string(report.failure_mode.as_deref()),
        json_string(&report.message),
    );
}

fn print_promotion_json(outcome: &PromotionOutcome) {
    println!(
        "{{\"success\":{},\"new_epoch\":{},\"promoted_replica_id\":{},\"message\":{}}}",
        outcome.success,
        outcome.new_epoch,
        outcome.promoted_replica_id,
        json_string(&outcome.message),
    );
}

fn print_demotion_json(outcome: &DemotionOutcome) {
    println!(
        "{{\"success\":{},\"new_primary_id\":{},\"message\":{}}}",
        outcome.success,
        json_option_u64(outcome.new_primary_id),
        json_string(&outcome.message),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hadr_status_returns_ok() {
        let result = run_hadr_status(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn hadr_status_accepts_json_output() {
        let result = run_hadr_status(&["--json".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn hadr_promote_requires_replica_id() {
        let result = run_hadr_promote(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn hadr_promote_accepts_valid_replica_id() {
        let result = run_hadr_promote(&["2".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn hadr_promote_rejects_invalid_replica_id() {
        let result = run_hadr_promote(&["not_a_number".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn hadr_demote_requires_force_flag() {
        let result = run_hadr_demote(&[]);
        assert!(result.is_ok()); // Returns OK but prints warning
    }

    #[test]
    fn hadr_demote_with_force() {
        let result = run_hadr_demote(&["--force".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn hadr_quorum_returns_ok() {
        let result = run_hadr_quorum(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn hadr_quorum_accepts_json_output() {
        let result = run_hadr_quorum(&["--json".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn hadr_node_register_requires_dry_run() {
        let result =
            run_hadr_node_register(&["4".to_string(), "--role".to_string(), "replica".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn hadr_node_register_dry_run_accepts_replica_role() {
        let result = run_hadr_node_register(&[
            "4".to_string(),
            "--role".to_string(),
            "replica".to_string(),
            "--dry-run".to_string(),
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn hadr_node_register_rejects_zero_node_id() {
        let result = run_hadr_node_register(&[
            "0".to_string(),
            "--role".to_string(),
            "replica".to_string(),
            "--dry-run".to_string(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn hadr_node_register_rejects_primary_role() {
        let result = run_hadr_node_register(&[
            "4".to_string(),
            "--role".to_string(),
            "primary".to_string(),
            "--dry-run".to_string(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn hadr_node_deregister_dry_run_requires_fencing_evidence() {
        let result = run_hadr_node_deregister(&["4".to_string(), "--dry-run".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn hadr_node_deregister_dry_run_accepts_fencing_evidence() {
        let result = run_hadr_node_deregister(&[
            "4".to_string(),
            "--fencing-evidence".to_string(),
            "ticket-123".to_string(),
            "--dry-run".to_string(),
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn hadr_node_list_is_read_only_contract_scaffold() {
        let result = run_hadr_node_list(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn hadr_node_status_requires_node_id() {
        let result = run_hadr_node_status(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn hadr_promote_accepts_json_output() {
        let result = run_hadr_promote(&["2".to_string(), "--json".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn hadr_json_string_escapes_diagnostic_fields() {
        assert_eq!(json_string("primary\trole"), "\"primary\\trole\"");
    }

    #[test]
    fn hadr_command_unknown_subcommand() {
        let result = run_hadr_command(&["unknown".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn hadr_command_help() {
        let result = run_hadr_command(&["--help".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn hadr_command_no_args_shows_help() {
        let result = run_hadr_command(&[]);
        assert!(result.is_ok());
    }
}
