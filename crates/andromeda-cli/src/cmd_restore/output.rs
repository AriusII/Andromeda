use super::{
    RestoreReplaySegmentOutput, RestoreStartOutcome, RestoreState, RestoreStatusReport,
    RestoreVerifyOutcome,
};
use crate::diagnostic_json::{json_option_string, json_option_u64, json_string};

pub(super) fn print_restore_help() {
    println!("Andromeda restore administration commands");
    println!();
    println!("USAGE: andromeda-cli restore <SUBCOMMAND> | restore <backup-id> [OPTIONS]");
    println!();
    println!("FORMS:");
    println!("  restore <backup-id> --artifact <dir> --pitr-lsn <lsn> --dry-run");
    println!(
        "                                      Validate restore preflight from backup artifact"
    );
    println!("  restore <backup-id> --artifact <dir> --pitr-policy latest --dry-run");
    println!(
        "                                      Select the artifact WAL end through an explicit policy"
    );
    println!(
        "  restore verify <backup-id> --artifact <dir> (--pitr-lsn <lsn>|--pitr-policy latest)"
    );
    println!("                                      Verify artifact preflight and replay plan");
    println!("  restore status <restore-id>        Show restore progress and state");
    println!();
    println!("OPTIONS:");
    println!("  --artifact <dir>                   File-backed backup artifact root");
    println!("  --pitr-lsn <lsn>                   Target LSN for point-in-time recovery");
    println!("  --pitr-policy latest               Explicitly restore to artifact WAL end");
    println!("  --validation-policy <full|minimal> Restore validation policy (default: full)");
    println!(
        "  --dry-run                          Validate mutating restore contract without orchestration"
    );
    println!("  --json                            Emit diagnostic machine-readable JSON output");
    println!("  -h, --help                         Show this help message");
    println!();
    println!(
        "STATES: {}",
        RestoreState::ALL
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
}

pub(super) fn print_restore_start(outcome: &RestoreStartOutcome, json_output: bool) {
    if json_output {
        print_restore_start_json(outcome);
    } else {
        print_restore_start_human(outcome);
    }
}

pub(super) fn print_restore_verify(outcome: &RestoreVerifyOutcome, json_output: bool) {
    if json_output {
        print_restore_verify_json(outcome);
    } else {
        print_restore_verify_human(outcome);
    }
}

pub(super) fn print_restore_status(report: &RestoreStatusReport, json_output: bool) {
    if json_output {
        print_restore_status_json(report);
    } else {
        print_restore_status_human(report);
    }
}

fn print_restore_start_human(outcome: &RestoreStartOutcome) {
    println!("Restore Contract Preview");
    println!("========================");
    println!("{}", outcome.message);
    println!("Contract Preview: {}", outcome.contract_preview);
    println!("Durable Backend: {}", outcome.durable_backend);
    println!(
        "Requires Restore Orchestrator: {}",
        outcome.requires_restore_orchestrator
    );
    println!("Dry Run: {}", outcome.dry_run);
    println!("Would Restore: {}", outcome.would_restore);
    println!("Backup ID: {}", outcome.backup_id);
    println!("Artifact Path: {}", outcome.artifact_path);
    if let Some(lsn) = outcome.pitr_target_lsn {
        println!("PITR Target LSN: {}", lsn);
    }
    if let Some(policy) = &outcome.pitr_policy {
        println!("PITR Policy: {policy}");
    }
    println!("Validation Policy: {}", outcome.validation_policy);
    println!("Preflight Validated: {}", outcome.preflight_validated);
    if !outcome.replay_segments.is_empty() {
        println!("Replay Segments:");
        for segment in &outcome.replay_segments {
            println!(
                "  - #{} segment_id={} lsn={}..={} contains_pitr={}",
                segment.sequence_index,
                segment.segment_id,
                segment.first_lsn,
                segment.last_lsn,
                segment.contains_pitr_target
            );
        }
    }
}

fn print_restore_status_human(report: &RestoreStatusReport) {
    println!("Restore Status Report");
    println!("====================");
    println!("Restore ID: {}", report.restore_id);
    println!("Backup ID: {}", report.backup_id);
    println!("Contract Preview: true");
    println!("Durable Backend: false");
    println!("Durable State Loaded: false");
    println!("Requires Restore Orchestrator: true");
    println!("Runtime Mode: contract_preview/static_restore_orchestrator");
    println!("State: {}", report.state);
    if let Some(pitr_lsn) = report.pitr_target_lsn {
        println!("PITR Target LSN: {}", pitr_lsn);
    }
    println!("Progress: {}%", report.progress_percent);
    println!("Start Time: {}", report.start_time);
    println!(
        "WAL Segments: {} / {}",
        report.wal_segments_replayed, report.estimated_total_segments
    );
    println!("Elapsed: {}s", report.elapsed_seconds);
}

fn print_restore_start_json(outcome: &RestoreStartOutcome) {
    println!(
        "{{\"schema\":\"andromeda.cli.restore.start.v1\",\"contract_preview\":{},\"durable_backend\":{},\"requires_restore_orchestrator\":{},\"dry_run\":{},\"would_restore\":{},\"restore_id\":{},\"backup_id\":{},\"artifact_path\":{},\"pitr_target_lsn\":{},\"pitr_policy\":{},\"validation_policy\":{},\"preflight_validated\":{},\"replay_segments\":{},\"message\":{}}}",
        outcome.contract_preview,
        outcome.durable_backend,
        outcome.requires_restore_orchestrator,
        outcome.dry_run,
        outcome.would_restore,
        outcome
            .restore_id
            .map(|restore_id| restore_id.to_string())
            .unwrap_or_else(|| "null".to_string()),
        outcome.backup_id,
        json_option_string(Some(outcome.artifact_path.as_str())),
        json_option_u64(outcome.pitr_target_lsn),
        json_option_string(outcome.pitr_policy.as_deref()),
        json_string(&outcome.validation_policy),
        outcome.preflight_validated,
        restore_replay_segments_json(&outcome.replay_segments),
        json_string(&outcome.message),
    );
}

fn print_restore_status_json(report: &RestoreStatusReport) {
    println!(
        "{{\"schema\":\"andromeda.cli.restore.status.v1\",\"contract_preview\":true,\"durable_backend\":false,\"durable_state_loaded\":false,\"runtime_mode\":\"contract_preview/static_restore_orchestrator\",\"requires_restore_orchestrator\":true,\"restore_id\":{},\"state\":{},\"backup_id\":{},\"pitr_target_lsn\":{},\"progress_percent\":{},\"wal_segments_replayed\":{},\"estimated_total_segments\":{},\"start_time\":{},\"elapsed_seconds\":{},\"message\":\"contract preview: durable restore status backend is not wired; showing static command contract only\"}}",
        report.restore_id,
        json_string(&report.state.to_string()),
        report.backup_id,
        json_option_u64(report.pitr_target_lsn),
        report.progress_percent,
        report.wal_segments_replayed,
        report.estimated_total_segments,
        report.start_time,
        report.elapsed_seconds,
    );
}

fn print_restore_verify_human(outcome: &RestoreVerifyOutcome) {
    println!("Restore Verify");
    println!("==============");
    println!("Backup ID: {}", outcome.backup_id);
    println!("Artifact Path: {}", outcome.artifact_path);
    println!("PITR Target LSN: {}", outcome.pitr_target_lsn);
    if let Some(policy) = &outcome.pitr_policy {
        println!("PITR Policy: {policy}");
    }
    println!("Validation Policy: {}", outcome.validation_policy);
    println!("Source Checkpoint LSN: {}", outcome.source_checkpoint_lsn);
    println!("Replay Segments:");
    for segment in &outcome.replay_segments {
        println!(
            "  - #{} segment_id={} lsn={}..={} contains_pitr={}",
            segment.sequence_index,
            segment.segment_id,
            segment.first_lsn,
            segment.last_lsn,
            segment.contains_pitr_target
        );
    }
    println!("{}", outcome.message);
}

fn print_restore_verify_json(outcome: &RestoreVerifyOutcome) {
    println!(
        "{{\"schema\":\"andromeda.cli.restore.verify.v1\",\"durable_backend\":true,\"backup_id\":{},\"artifact_path\":{},\"pitr_target_lsn\":{},\"pitr_policy\":{},\"validation_policy\":{},\"source_checkpoint_lsn\":{},\"replay_segments\":{},\"message\":{}}}",
        outcome.backup_id,
        json_string(&outcome.artifact_path),
        outcome.pitr_target_lsn,
        json_option_string(outcome.pitr_policy.as_deref()),
        json_string(&outcome.validation_policy),
        outcome.source_checkpoint_lsn,
        restore_replay_segments_json(&outcome.replay_segments),
        json_string(&outcome.message),
    );
}

fn restore_replay_segments_json(segments: &[RestoreReplaySegmentOutput]) -> String {
    let entries = segments
        .iter()
        .map(|segment| {
            format!(
                "{{\"sequence_index\":{},\"segment_id\":{},\"first_lsn\":{},\"last_lsn\":{},\"contains_pitr_target\":{}}}",
                segment.sequence_index,
                segment.segment_id,
                segment.first_lsn,
                segment.last_lsn,
                segment.contains_pitr_target
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{}]", entries)
}
