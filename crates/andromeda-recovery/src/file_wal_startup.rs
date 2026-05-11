//! File-WAL startup recovery DTOs and orchestration shared by storage integrations.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_segment::segment_index::SegmentIndexV0;
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

/// Plan startup from durable manifest evidence, an optional durable segment-index
/// byte slice, and the file-WAL durable prefix.
///
/// # Ordering guarantee (WAL-before-commit)
///
/// The segment index bytes are decoded and validated **before** the WAL file is
/// opened. If `segment_index_bytes` contains corrupt or incompatible data the
/// function returns `Err` without touching the WAL file. This prevents a
/// partially-constructed runtime from observing cold-snapshot state before the
/// segment catalog is confirmed consistent.
///
/// # Bootstrap mode
///
/// Pass `segment_index_bytes = None` when the segment index has not yet been
/// written (first start, clean install). In that case `evidence.segment_index_validated`
/// is set to `true` immediately — the absence of a segment index is a valid
/// bootstrap condition.
pub fn plan_file_wal_startup_recovery_v0_with_segment_index(
    manifest: &impl RecoveryManifestView,
    startup_mode: StartupMode,
    wal_path: impl AsRef<Path>,
    segment_index_bytes: Option<&[u8]>,
    forensic_report_attached: bool,
) -> AndromedaResult<FileWalStartupRecoveryV0> {
    // ── Step 1: validate segment index BEFORE WAL scan ───────────────────────
    let segment_index_validated = match segment_index_bytes {
        None => true, // bootstrap: no segment index yet
        Some(bytes) => {
            SegmentIndexV0::decode(bytes).map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!("segment index decode failed before WAL scan: {e}"),
                )
            })?;
            true
        },
    };

    // ── Step 2: scan the WAL (only reached when segment index is valid) ───────
    let disk_scan = andromeda_wal::scan_file_wal(wal_path)?;

    // ── Step 3: build evidence with explicit segment_index_validated flag ─────
    let mut evidence = StartupEvidence::from_manifest_and_wal_scan(
        manifest,
        &disk_scan.scan,
        forensic_report_attached,
    );
    evidence.segment_index_validated = segment_index_validated;

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
