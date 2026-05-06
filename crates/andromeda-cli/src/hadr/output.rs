use crate::diagnostic_json::{
    json_option_string, json_option_u64, json_string, json_string_array, json_u64_array,
};

use super::types::{
    DemotionOutcome, FailoverPrepareReport, HadrStatusReport, NodeManagementReport,
    PromotionOutcome, QuorumStatusReport, ReplicaStatus,
};

pub(super) fn print_hadr_help() {
    println!("Andromeda HADR administration commands");
    println!();
    println!("USAGE: andromeda-cli hadr <SUBCOMMAND> [OPTIONS]");
    println!();
    println!("SUBCOMMANDS:");
    println!("  status                Display quorum membership, LSN state, replica lag");
    println!(
        "  node <command>        Node management contract commands (register/deregister/list/status)"
    );
    println!("  promote <replica-id>  Promote replica to primary");
    println!("  demote                Demote current primary to replica (planned failover)");
    println!("  failover-prepare      Pre-validate failover safety without state change (dry-run)");
    println!("  quorum                Show quorum configuration and fencing status");
    println!();
    println!("OPTIONS:");
    println!("  --force               Confirm destructive operations (e.g., demote)");
    println!("  --witness-check       Verify witness availability for failover-prepare");
    println!("  --json                Emit diagnostic machine-readable JSON output");
    println!("  -h, --help            Show this help message");
}

pub(super) fn print_hadr_node_help() {
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

pub(super) fn print_hadr_status(report: &HadrStatusReport, json_output: bool) {
    if json_output {
        print_hadr_status_json(report);
    } else {
        print_hadr_status_human(report);
    }
}

pub(super) fn print_quorum_status(report: &QuorumStatusReport, json_output: bool) {
    if json_output {
        print_quorum_status_json(report);
    } else {
        print_quorum_status_human(report);
    }
}

pub(super) fn print_node_report(report: &NodeManagementReport, json_output: bool) {
    if json_output {
        print_node_report_json(report);
    } else {
        print_node_report_human(report);
    }
}

pub(super) fn print_promotion_outcome(outcome: &PromotionOutcome, json_output: bool) {
    if json_output {
        print_promotion_json(outcome);
    } else if outcome.success {
        println!(
            "✓ Replica {} promoted to primary (epoch {})",
            outcome.promoted_replica_id, outcome.new_epoch
        );
    } else {
        println!("✗ Promotion failed: {}", outcome.message);
    }
}

pub(super) fn print_demotion_outcome(outcome: &DemotionOutcome, json_output: bool) {
    if json_output {
        print_demotion_json(outcome);
    } else if outcome.success {
        if let Some(new_primary) = outcome.new_primary_id {
            println!("✓ Primary demoted; replica {} is new primary", new_primary);
        } else {
            println!("✓ Primary demoted (no suitable replica for immediate promotion)");
        }
    } else {
        println!("✗ Demotion failed: {}", outcome.message);
    }
}

pub(super) fn print_demotion_force_warning(json_output: bool) {
    if json_output {
        println!(
            "{{\"success\":false,\"new_primary_id\":null,\"message\":{}}}",
            json_string("Demotion requires --force confirmation")
        );
    } else {
        println!("⚠ WARNING: Demoting primary will disrupt writes.");
        println!("Use --force to confirm.");
    }
}

pub(super) fn print_failover_prepare(report: &FailoverPrepareReport, json_output: bool) {
    if json_output {
        print_failover_prepare_json(report);
    } else {
        print_failover_prepare_human(report);
    }
}

pub(super) fn print_hadr_status_human(report: &HadrStatusReport) {
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

pub(super) fn print_quorum_status_human(report: &QuorumStatusReport) {
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

fn print_failover_prepare_human(report: &FailoverPrepareReport) {
    println!("Failover Readiness Assessment");
    println!("=============================");
    println!(
        "Ready for Failover: {}",
        if report.ready_for_failover {
            "✓ YES"
        } else {
            "✗ NO"
        }
    );
    println!("Current Epoch: {}", report.current_epoch);
    println!(
        "Quorum Size: {}/{}",
        report.quorum_size,
        report.quorum_size * 2 - 1
    );
    println!(
        "Witness Available: {}",
        if report.witness_available {
            "✓ YES"
        } else {
            "✗ NO"
        }
    );
    if let Some(candidate_id) = report.promotion_candidate_id {
        println!("Promotion Candidate: Replica {}", candidate_id);
        if let Some(distance) = report.promotion_candidate_lsn_distance {
            println!("  LSN Distance (bytes): {}", distance);
        }
    }
    println!("Fencing Policy: {}", report.fencing_policy);
    println!();
    if !report.blocking_issues.is_empty() {
        println!("Blocking Issues:");
        for issue in &report.blocking_issues {
            println!("  ✗ {}", issue);
        }
        println!();
    }
    if !report.remediation_steps.is_empty() {
        println!("Remediation Steps:");
        for step in &report.remediation_steps {
            println!("  → {}", step);
        }
        println!();
    }
    println!("{}", report.message);
}

fn print_failover_prepare_json(report: &FailoverPrepareReport) {
    println!(
        "{{\"ready_for_failover\":{},\"current_epoch\":{},\"quorum_size\":{},\"witness_available\":{},\"promotion_candidate_id\":{},\"promotion_candidate_lsn_distance\":{},\"blocking_issues\":{},\"remediation_steps\":{},\"fencing_policy\":{},\"message\":{}}}",
        report.ready_for_failover,
        report.current_epoch,
        report.quorum_size,
        report.witness_available,
        json_option_u64(report.promotion_candidate_id),
        json_option_u64(report.promotion_candidate_lsn_distance.map(|x| x as u64)),
        json_string_array(&report.blocking_issues),
        json_string_array(&report.remediation_steps),
        json_string(&report.fencing_policy),
        json_string(&report.message),
    );
}
