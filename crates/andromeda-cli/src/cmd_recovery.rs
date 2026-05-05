//! Recovery inspection command.

use crate::args::RecoveryInspectOptions;
use andromeda_core::AndromedaResult;
use andromeda_storage::{
    DatabaseManifest, FileWalRecoveryReportV0, Lsn, StartupMode, report_file_wal_recovery_v0,
};
use std::path::Path;

/// Runs recovery inspection for a WAL file.
pub fn run_recovery_inspect(options: RecoveryInspectOptions) -> AndromedaResult<()> {
    let report = recovery_report_for_path(&options.wal_path, options.required_wal_start_lsn)?;
    print!("{}", recovery_inspect_report(&options.wal_path, &report));
    Ok(())
}

/// Generates a recovery report for a WAL path.
pub fn recovery_report_for_path(
    wal_path: &Path,
    required_wal_start_lsn: Lsn,
) -> AndromedaResult<FileWalRecoveryReportV0> {
    let manifest = v0_demo_manifest(required_wal_start_lsn);
    report_file_wal_recovery_v0(&manifest, StartupMode::SafeStart, wal_path)
}

/// Generates a recovery inspection report.
pub fn recovery_inspect_report(wal_path: &Path, report: &FileWalRecoveryReportV0) -> String {
    let replay_lsns = report
        .replay_lsns()
        .map(|lsn| lsn.get())
        .collect::<Vec<_>>();
    let ignored_transactions = report
        .ignored_transaction_ids()
        .map(|id| id.get())
        .collect::<Vec<_>>();
    let scan_stop = report
        .scan_stop
        .map(|stop| format!("{:?} at byte {}", stop.reason, stop.offset))
        .unwrap_or_else(|| "none".to_string());

    let mut output = String::new();
    output.push_str("Andromeda V0 WAL recovery inspect\n");
    output.push_str(&format!("WAL path: {}\n", wal_path.display()));
    output.push_str(&format!("startup mode: {:?}\n", report.startup_mode));
    output.push_str(&format!(
        "format version: {}\n",
        report.header.format_version
    ));
    output.push_str("byte order: little-endian\n");
    output.push_str(&format!("segment id: {}\n", report.header.segment_id));
    output.push_str(&format!(
        "physical WAL bytes: {}\n",
        report.physical_wal_bytes
    ));
    output.push_str(&format!("scanned bytes: {}\n", report.scanned_bytes));
    output.push_str(&format!(
        "durable prefix bytes: {}\n",
        report.durable_prefix_bytes
    ));
    output.push_str(&format!(
        "durable prefix records: {}\n",
        report.durable_prefix_record_count
    ));
    output.push_str(&format!("durable LSN: {}\n", report.durable_lsn.get()));
    output.push_str(&format!("boundary: {:?}\n", report.boundary_kind));
    output.push_str(&format!("scan stop: {scan_stop}\n"));
    output.push_str(&format!(
        "forensic required: {}\n",
        report.forensic_required
    ));
    output.push_str(&format!("replay LSNs: {:?}\n", replay_lsns));
    output.push_str(&format!(
        "ignored transactions: {:?}\n",
        ignored_transactions
    ));
    output.push_str(&format!(
        "ignored records: {}\n",
        report.ignored_record_count
    ));
    output
}

fn v0_demo_manifest(required_wal_start_lsn: Lsn) -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_demo_manifest_is_valid() {
        let manifest = v0_demo_manifest(Lsn::new(1));
        assert_eq!(manifest.database_id, 1);
        assert_eq!(manifest.required_wal_start_lsn, Lsn::new(1));
    }
}
