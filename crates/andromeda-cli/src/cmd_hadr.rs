//! HADR administration commands.
//!
//! Provides CLI commands for:
//! - Quorum membership and status inspection
//! - Replica promotion and demotion
//! - Fencing policy configuration
//! - LSN gap and lag visibility

use crate::error::cli_error;
use andromeda_core::AndromedaResult;

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

/// Displays HADR status including quorum membership, LSN state, and replica lag.
fn run_hadr_status(args: &[String]) -> AndromedaResult<()> {
    let json_output = has_json_option(args);

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

    let json_output = has_json_option(&args[1..]);

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
    let json_output = has_json_option(args);
    let force = args.iter().any(|arg| arg == "--force");

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
    let json_output = has_json_option(args);

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
    println!("  promote <replica-id>   Promote replica to primary");
    println!("  demote              Demote current primary to replica (planned failover)");
    println!("  quorum              Show quorum configuration and fencing status");
    println!();
    println!("OPTIONS:");
    println!("  --force             Confirm destructive operations (e.g., demote)");
    println!("  --json              Emit diagnostic machine-readable JSON output");
    println!("  -h, --help          Show this help message");
}

fn has_json_option(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--json")
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
        "{{\"total_members\":{},\"quorum_size\":{},\"member_ids\":[{}],\"fencing_policy\":{},\"fencing_status\":{}}}",
        report.total_members,
        report.quorum_size,
        report
            .member_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(","),
        json_string(&report.fencing_policy),
        json_string(&report.fencing_status),
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

fn json_option_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn json_string(value: &str) -> String {
    format!("\"{}\"", escape_json_str(value))
}

fn escape_json_str(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
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
