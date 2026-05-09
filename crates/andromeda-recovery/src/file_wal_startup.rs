//! File-WAL startup recovery DTOs and orchestration shared by storage integrations.

use andromeda_error::AndromedaResult;
use andromeda_wal::{FileWalDiskScan, WalRecord};
use std::path::Path;

use crate::{
    ConceptualRedoPlan, RecoveryManifestView, RecoveryPlan, StartupAuditProjection,
    StartupDecision, StartupEvidence, StartupMode, decide_startup,
};

/// File-WAL boot/recovery orchestration result for the V0 vertical slice.
///
/// The struct is an audit-oriented boundary: it is built only from validated
/// manifest projection plus durable WAL scan evidence. It does not apply redo
/// and does not claim that reconstructed RAM is truth.
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

    pub const fn audit_projection<TraceId>(
        &self,
        trace_id: TraceId,
    ) -> StartupAuditProjection<TraceId>
    where
        TraceId: Copy,
    {
        self.decision.audit_projection(trace_id)
    }

    /// The floor to pass to a transaction id allocator before post-recovery
    /// traffic.
    pub const fn transaction_manager_allocator_floor(&self) -> u64 {
        self.recovered_transaction_id_floor
    }
}

/// Highest transaction id observed in durable WAL evidence.
pub fn recovered_transaction_id_floor_from_records(records: &[WalRecord]) -> u64 {
    records
        .iter()
        .filter_map(|record| record.header.transaction_id)
        .map(|transaction_id| transaction_id.get())
        .max()
        .unwrap_or(0)
}

/// Build a conceptual redo plan from a durable file-WAL scan.
pub fn recover_from_file_wal(
    manifest: &impl RecoveryManifestView,
    startup_mode: StartupMode,
    path: impl AsRef<Path>,
) -> AndromedaResult<ConceptualRedoPlan> {
    let disk_scan = andromeda_wal::scan_file_wal(path)?;
    RecoveryPlan::from_manifest_and_wal_scan(manifest, startup_mode, &disk_scan.scan)
}

/// Plan startup from durable manifest evidence plus the file-WAL durable prefix.
pub fn plan_file_wal_startup_recovery_v0(
    manifest: &impl RecoveryManifestView,
    startup_mode: StartupMode,
    path: impl AsRef<Path>,
    forensic_report_attached: bool,
) -> AndromedaResult<FileWalStartupRecoveryV0> {
    let disk_scan = andromeda_wal::scan_file_wal(path)?;
    plan_file_wal_startup_recovery_from_scan_v0(
        manifest,
        startup_mode,
        disk_scan,
        forensic_report_attached,
    )
}

/// Plan startup from an already collected file-WAL scan.
pub fn plan_file_wal_startup_recovery_from_scan_v0(
    manifest: &impl RecoveryManifestView,
    startup_mode: StartupMode,
    disk_scan: FileWalDiskScan,
    forensic_report_attached: bool,
) -> AndromedaResult<FileWalStartupRecoveryV0> {
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
