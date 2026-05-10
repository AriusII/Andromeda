use crate::diagnostic_json::{
    json_option_string, json_option_u64, json_string, json_string_array, json_u64_array,
};

use super::types::{
    DemotionOutcome, FailoverPrepareReport, HadrStatusReport, NodeManagementReport,
    NodeMembershipMemberReport, PromotionOutcome, QuorumStatusReport, ReplicaStatus,
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
    println!("  promote <replica-id>  Validate or apply replica promotion");
    println!("  demote                Preview primary demotion with --force --dry-run");
    println!("  failover-prepare      Pre-validate failover safety without state change (dry-run)");
    println!("  quorum                Show quorum configuration and fencing status");
    println!();
    println!("OPTIONS:");
    println!("  --membership-store <file>  Use a durable HADR membership store");
    println!("  --state-dir <dir>          Use <dir>/hadr-membership.bin as membership store");
    println!("  --force               Confirm destructive operations (e.g., demote)");
    println!("  --apply               Apply a durable HADR mutation when a backend is provided");
    println!("  --dry-run             Validate a mutating contract without applying it");
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
    println!("  register <node-id> --role replica --dry-run|--apply");
    println!("  deregister <node-id> --fencing-evidence <evidence-id> --dry-run|--apply");
    println!("  fence <node-id> --fencing-evidence <evidence-id> --dry-run|--apply");
    println!("  list");
    println!("  status <node-id>");
    println!();
    println!("OPTIONS:");
    println!("  --membership-store <file>  Use a durable HADR membership store");
    println!("  --state-dir <dir>          Use <dir>/hadr-membership.bin as membership store");
    println!("  --apply             Apply the durable membership mutation");
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
        print_runtime_contract_header(
            outcome.contract_preview,
            outcome.durable_backend,
            outcome.dry_run,
        );
        println!("Dry Run: {}", outcome.dry_run);
        println!("Would Apply: {}", outcome.would_apply);
        if outcome.dry_run {
            println!(
                "Replica {} promotion preview accepted (next epoch {})",
                outcome.promoted_replica_id, outcome.new_epoch
            );
        } else {
            println!(
                "Replica {} promoted to primary (epoch {})",
                outcome.promoted_replica_id, outcome.new_epoch
            );
        }
        if let Some(committed_safe_lsn) = outcome.committed_safe_lsn {
            println!("Committed Safe LSN: {}", committed_safe_lsn);
        }
        if let Some(quorum_size) = outcome.quorum_size {
            println!("Quorum Size: {}", quorum_size);
        }
        if let Some(granted_votes) = outcome.granted_votes {
            println!("Granted Votes: {}", granted_votes);
        }
        if let (Some(primary_id), Some(epoch)) = (outcome.fencing_primary_id, outcome.fencing_epoch)
        {
            println!("Fencing Evidence: primary={} epoch={}", primary_id, epoch);
        }
        if let Some(audit_log) = &outcome.audit_log {
            println!("Promotion Audit Log: {}", audit_log);
        }
        println!("{}", outcome.message);
    } else {
        println!("✗ Promotion failed: {}", outcome.message);
    }
}

pub(super) fn print_demotion_outcome(outcome: &DemotionOutcome, json_output: bool) {
    if json_output {
        print_demotion_json(outcome);
    } else if outcome.success {
        println!("Contract Preview: {}", outcome.contract_preview);
        println!("Dry Run: {}", outcome.dry_run);
        println!("Would Apply: {}", outcome.would_apply);
        if let Some(new_primary) = outcome.new_primary_id {
            println!(
                "Primary demotion preview accepted; replica {} would become new primary",
                new_primary
            );
        } else {
            println!(
                "Primary demotion preview accepted (no suitable replica for immediate promotion)"
            );
        }
        println!("{}", outcome.message);
    } else {
        println!("✗ Demotion failed: {}", outcome.message);
    }
}

pub(super) fn print_demotion_force_warning(json_output: bool) {
    if json_output {
        println!(
            "{{\"success\":false,\"dry_run\":false,\"would_apply\":false,\"contract_preview\":true,\"new_primary_id\":null,\"message\":{}}}",
            json_string("Demotion requires --force confirmation")
        );
    } else {
        println!("Contract Preview: true");
        println!("Dry Run: false");
        println!("Would Apply: false");
        println!("⚠ WARNING: Demoting primary will disrupt writes.");
        println!("Use --force --dry-run to confirm the contract preview.");
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
    println!("Contract Preview: {}", report.contract_preview);
    println!("Durable Backend Wired: {}", report.durable_backend);
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
    println!("{}", report.message);
}

pub(super) fn print_quorum_status_human(report: &QuorumStatusReport) {
    println!("Quorum Configuration");
    println!("====================");
    println!("Contract Preview: {}", report.contract_preview);
    println!("Durable Backend Wired: {}", report.durable_backend);
    println!("Total Members: {}", report.total_members);
    println!("Quorum Size: {}", report.quorum_size);
    println!("Member IDs: {:?}", report.member_ids);
    println!("Fencing Policy: {}", report.fencing_policy);
    println!("Fencing Status: {}", report.fencing_status);
    println!("{}", report.message);
}

fn print_hadr_status_json(report: &HadrStatusReport) {
    println!(
        "{{\"contract_preview\":{},\"durable_backend\":{},\"cluster_role\":{},\"epoch\":{},\"current_primary\":{},\"replicas\":{},\"durable_lsn\":{},\"committed_lsn\":{},\"message\":{}}}",
        report.contract_preview,
        report.durable_backend,
        json_string(&report.cluster_role),
        report.epoch,
        json_option_u64(report.current_primary),
        replicas_json(&report.replicas),
        report.durable_lsn,
        report.committed_lsn,
        json_string(&report.message),
    );
}

fn replicas_json(replicas: &[ReplicaStatus]) -> String {
    json_array(replicas, |replica| {
        format!(
            "{{\"replica_id\":{},\"health_state\":{},\"received_lsn\":{},\"shipped_lsn\":{},\"lag_bytes\":{}}}",
            replica.replica_id,
            json_string(&replica.health_state),
            replica.received_lsn,
            replica.shipped_lsn,
            replica.lag_bytes,
        )
    })
}

fn print_quorum_status_json(report: &QuorumStatusReport) {
    println!(
        "{{\"contract_preview\":{},\"durable_backend\":{},\"total_members\":{},\"quorum_size\":{},\"member_ids\":{},\"fencing_policy\":{},\"fencing_status\":{},\"message\":{}}}",
        report.contract_preview,
        report.durable_backend,
        report.total_members,
        report.quorum_size,
        json_u64_array(&report.member_ids),
        json_string(&report.fencing_policy),
        json_string(&report.fencing_status),
        json_string(&report.message),
    );
}

fn print_node_report_human(report: &NodeManagementReport) {
    println!("HADR Node Management Contract");
    println!("=============================");
    print_runtime_contract_header(
        report.contract_preview,
        report.durable_backend,
        report.dry_run,
    );
    println!("Action: {}", report.action);
    if let Some(node_id) = report.node_id {
        println!("Node ID: {}", node_id);
    }
    if let Some(role) = &report.role {
        println!("Role: {}", role);
    }
    if let Some(epoch) = report.membership_epoch {
        println!("Membership Epoch: {}", epoch);
    }
    if !report.members.is_empty() {
        println!("Members:");
        for member in &report.members {
            println!(
                "  Node {} ({}) role_epoch={}",
                member.node_id, member.role, member.role_epoch
            );
        }
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
        "{{\"contract_preview\":{},\"durable_backend\":{},\"runtime_mode\":{},\"action\":{},\"node_id\":{},\"role\":{},\"membership_epoch\":{},\"members\":{},\"dry_run\":{},\"would_apply\":{},\"quorum_check\":{},\"fencing_check\":{},\"audit_event\":{},\"required_permissions\":{},\"failure_mode\":{},\"message\":{}}}",
        report.contract_preview,
        report.durable_backend,
        json_string(runtime_mode(
            report.contract_preview,
            report.durable_backend,
            report.dry_run,
        )),
        json_string(&report.action),
        json_option_u64(report.node_id),
        json_option_string(report.role.as_deref()),
        json_option_u64(report.membership_epoch),
        node_members_json(&report.members),
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

fn node_members_json(members: &[NodeMembershipMemberReport]) -> String {
    json_array(members, |member| {
        format!(
            "{{\"node_id\":{},\"role\":{},\"role_epoch\":{}}}",
            member.node_id,
            json_string(&member.role),
            member.role_epoch,
        )
    })
}

fn print_promotion_json(outcome: &PromotionOutcome) {
    println!(
        "{{\"success\":{},\"dry_run\":{},\"would_apply\":{},\"contract_preview\":{},\"durable_backend\":{},\"runtime_mode\":{},\"new_epoch\":{},\"promoted_replica_id\":{},\"candidate_lsn\":{},\"audit_lsn\":{},\"committed_safe_lsn\":{},\"quorum_size\":{},\"granted_votes\":{},\"fencing_primary_id\":{},\"fencing_epoch\":{},\"audit_log\":{},\"message\":{}}}",
        outcome.success,
        outcome.dry_run,
        outcome.would_apply,
        outcome.contract_preview,
        outcome.durable_backend,
        json_string(runtime_mode(
            outcome.contract_preview,
            outcome.durable_backend,
            outcome.dry_run,
        )),
        outcome.new_epoch,
        outcome.promoted_replica_id,
        json_option_u64(outcome.candidate_lsn),
        json_option_u64(outcome.audit_lsn),
        json_option_u64(outcome.committed_safe_lsn),
        json_option_u64(outcome.quorum_size.map(|value| value as u64)),
        json_option_u64(outcome.granted_votes.map(|value| value as u64)),
        json_option_u64(outcome.fencing_primary_id),
        json_option_u64(outcome.fencing_epoch),
        json_option_string(outcome.audit_log.as_deref()),
        json_string(&outcome.message),
    );
}

fn runtime_mode(contract_preview: bool, durable_backend: bool, dry_run: bool) -> &'static str {
    match (contract_preview, durable_backend, dry_run) {
        (true, false, true) => "contract_scaffold_dry_run",
        (true, false, false) => "contract_scaffold",
        (_, true, true) => "durable_runtime_dry_run",
        (_, true, false) => "applied_durable_runtime",
        _ => "contract_scaffold",
    }
}

fn print_runtime_contract_header(contract_preview: bool, durable_backend: bool, dry_run: bool) {
    println!(
        "Runtime Mode: {}",
        runtime_mode(contract_preview, durable_backend, dry_run)
    );
    println!("Contract Preview: {}", contract_preview);
    println!("Durable Backend Wired: {}", durable_backend);
}

fn json_array<T>(items: &[T], render: impl FnMut(&T) -> String) -> String {
    let entries = items.iter().map(render).collect::<Vec<_>>().join(",");
    format!("[{}]", entries)
}

fn print_demotion_json(outcome: &DemotionOutcome) {
    println!(
        "{{\"success\":{},\"dry_run\":{},\"would_apply\":{},\"contract_preview\":{},\"new_primary_id\":{},\"message\":{}}}",
        outcome.success,
        outcome.dry_run,
        outcome.would_apply,
        outcome.contract_preview,
        json_option_u64(outcome.new_primary_id),
        json_string(&outcome.message),
    );
}

fn print_failover_prepare_human(report: &FailoverPrepareReport) {
    println!("Failover Readiness Assessment");
    println!("=============================");
    println!("Contract Preview: {}", report.contract_preview);
    println!("Dry Run: {}", report.dry_run);
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
        "{{\"dry_run\":{},\"contract_preview\":{},\"ready_for_failover\":{},\"current_epoch\":{},\"quorum_size\":{},\"witness_available\":{},\"promotion_candidate_id\":{},\"promotion_candidate_lsn_distance\":{},\"blocking_issues\":{},\"remediation_steps\":{},\"fencing_policy\":{},\"message\":{}}}",
        report.dry_run,
        report.contract_preview,
        report.ready_for_failover,
        report.current_epoch,
        report.quorum_size,
        report.witness_available,
        json_option_u64(report.promotion_candidate_id),
        json_option_i64(report.promotion_candidate_lsn_distance),
        json_string_array(&report.blocking_issues),
        json_string_array(&report.remediation_steps),
        json_string(&report.fencing_policy),
        json_string(&report.message),
    );
}

pub(super) fn json_option_i64(value: Option<i64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}
