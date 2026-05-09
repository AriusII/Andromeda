//! File-WAL startup recovery DTOs shared by storage integrations.

use andromeda_wal::{FileWalDiskScan, WalRecord};

use crate::{
    ConceptualRedoPlan, StartupAuditProjection, StartupDecision, StartupEvidence, StartupMode,
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
