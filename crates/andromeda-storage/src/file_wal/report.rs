use andromeda_core::AndromedaResult;
use andromeda_recovery::{
    FileWalRecoveryBoundaryKind, FileWalRecoveryReportV0, build_file_wal_recovery_report_v0,
    file_wal_recovery_boundary_kind,
};
use andromeda_wal::scan_file_wal;
use std::path::Path;

use crate::{DatabaseManifest, RecoveryPlan, StartupMode};

pub fn report_file_wal_recovery_v0(
    manifest: &DatabaseManifest,
    startup_mode: StartupMode,
    path: impl AsRef<Path>,
) -> AndromedaResult<FileWalRecoveryReportV0> {
    manifest.validate()?;
    let disk_scan = scan_file_wal(path)?;
    let boundary_kind = file_wal_recovery_boundary_kind(disk_scan.scan.stopped);
    let forensic_required = matches!(
        boundary_kind,
        FileWalRecoveryBoundaryKind::ForensicChainBreak
    );

    let redo_plan = if forensic_required {
        None
    } else {
        Some(RecoveryPlan::from_manifest_and_wal_scan(
            manifest,
            startup_mode,
            &disk_scan.scan,
        )?)
    };

    build_file_wal_recovery_report_v0(startup_mode, &disk_scan, redo_plan.as_ref())
}
