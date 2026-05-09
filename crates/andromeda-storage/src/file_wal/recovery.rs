use andromeda_core::AndromedaResult;
pub use andromeda_recovery::FileWalStartupRecoveryV0;
use andromeda_recovery::{FileWalRecoveryReportV0, recovered_transaction_id_floor_from_records};
use andromeda_wal::scan_file_wal;
use std::path::Path;

use crate::{
    ConceptualRedoPlan, DatabaseManifest, RecoveryPlan, StartupEvidence, StartupMode,
    decide_startup,
};

use super::report;

pub fn recover_from_file_wal(
    manifest: &DatabaseManifest,
    startup_mode: StartupMode,
    path: impl AsRef<Path>,
) -> AndromedaResult<ConceptualRedoPlan> {
    let disk_scan = scan_file_wal(path)?;
    RecoveryPlan::from_manifest_and_wal_scan(manifest, startup_mode, &disk_scan.scan)
}

pub fn plan_file_wal_startup_recovery_v0(
    manifest: &DatabaseManifest,
    startup_mode: StartupMode,
    path: impl AsRef<Path>,
    forensic_report_attached: bool,
) -> AndromedaResult<FileWalStartupRecoveryV0> {
    let disk_scan = scan_file_wal(path)?;
    let evidence = StartupEvidence::from_manifest_and_wal_scan(
        manifest,
        &disk_scan.scan,
        forensic_report_attached,
    );
    let decision = decide_startup(startup_mode, evidence);
    let recovered_transaction_id_floor =
        recovered_transaction_id_floor_from_records(&disk_scan.scan.records);

    let redo_plan = match decision.acceptance() {
        Some(acceptance) if acceptance.replay_allowed => Some(
            RecoveryPlan::from_manifest_and_wal_scan(manifest, startup_mode, &disk_scan.scan)?,
        ),
        _ => None,
    };

    Ok(FileWalStartupRecoveryV0 {
        startup_mode,
        disk_scan,
        evidence,
        decision,
        redo_plan,
        recovered_transaction_id_floor,
    })
}

pub fn report_file_wal_recovery_v0(
    manifest: &DatabaseManifest,
    startup_mode: StartupMode,
    path: impl AsRef<Path>,
) -> AndromedaResult<FileWalRecoveryReportV0> {
    report::report_file_wal_recovery_v0(manifest, startup_mode, path)
}
