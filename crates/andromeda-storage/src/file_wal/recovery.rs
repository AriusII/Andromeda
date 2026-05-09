use andromeda_core::AndromedaResult;
use andromeda_recovery::FileWalRecoveryReportV0;
use andromeda_wal::{FileWalDiskScan, scan_file_wal};
use std::path::Path;

use crate::{
    ConceptualRedoPlan, DatabaseManifest, RecoveryPlan, StartupAuditProjection, StartupDecision,
    StartupEvidence, StartupMode, WalRecord, decide_startup,
};

use super::report;

/// File-WAL boot/recovery orchestration result for the V0 vertical slice.
///
/// The struct is an audit-oriented seam: it is built only from a validated
/// manifest projection plus a durable WAL disk scan. It does not apply redo and
/// does not claim that reconstructed RAM is truth. Callers that accept
/// `redo_plan` are responsible for applying its `Replay` records to a mounted
/// cold snapshot, then constructing/seeding their transaction manager with
/// `recovered_transaction_id_floor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileWalStartupRecoveryV0 {
    pub startup_mode: StartupMode,
    pub disk_scan: FileWalDiskScan,
    pub evidence: StartupEvidence,
    pub decision: StartupDecision,
    pub redo_plan: Option<ConceptualRedoPlan>,
    pub recovered_transaction_id_floor: u64,
}

impl FileWalStartupRecoveryV0 {
    pub fn replay_allowed(&self) -> bool {
        matches!(
            self.decision.acceptance(),
            Some(acceptance) if acceptance.replay_allowed
        )
    }

    pub fn audit_projection(&self, trace_id: andromeda_observe::TraceId) -> StartupAuditProjection {
        self.decision.audit_projection(trace_id)
    }

    /// The floor to pass to `TransactionManager::with_recovered_floor` or
    /// `TransactionManager::seed_allocator` before post-recovery traffic.
    pub const fn transaction_manager_allocator_floor(&self) -> u64 {
        self.recovered_transaction_id_floor
    }
}

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

fn recovered_transaction_id_floor_from_records(records: &[WalRecord]) -> u64 {
    records
        .iter()
        .filter_map(|record| record.header.transaction_id)
        .map(|transaction_id| transaction_id.get())
        .max()
        .unwrap_or(0)
}
