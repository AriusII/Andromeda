use andromeda_observe::TraceId;

use super::super::planning::StartupMode;
use super::{
    ObservedBoundary, StartupAcceptance, StartupEvidence, StartupOutcome, StartupRejectionReason,
};

/// A fully-formed, auditable startup decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupDecision {
    pub mode: StartupMode,
    pub evidence: StartupEvidence,
    pub outcome: StartupOutcome,
}

impl StartupDecision {
    pub const fn is_accepted(&self) -> bool {
        self.outcome.is_accepted()
    }

    pub const fn rejection(&self) -> Option<StartupRejectionReason> {
        match self.outcome {
            StartupOutcome::Rejected(reason) => Some(reason),
            StartupOutcome::Accepted(_) => None,
        }
    }

    pub const fn acceptance(&self) -> Option<StartupAcceptance> {
        match self.outcome {
            StartupOutcome::Accepted(proof) => Some(proof),
            StartupOutcome::Rejected(_) => None,
        }
    }

    /// Project the decision into an audit-friendly struct that carries the
    /// mode, the durable evidence, and the outcome. Intended to be emitted
    /// by the observer plane alongside [`andromeda_observe::RecoveryTrace`].
    pub const fn audit_projection(&self, trace_id: TraceId) -> StartupAuditProjection {
        StartupAuditProjection {
            trace_id,
            mode: self.mode,
            mounted_snapshot_id: self.evidence.mounted_snapshot_id,
            required_wal_start_lsn: self.evidence.required_wal_start_lsn,
            last_durable_lsn: self.evidence.last_durable_lsn,
            observed_boundary: self.evidence.observed_boundary(),
            forensic_report_attached: self.evidence.forensic_report_attached,
            outcome: self.outcome,
        }
    }

    /// Project the decision into the observer-plane recovery trace, only
    /// when the decision was accepted. Rejections must be audited via
    /// [`Self::audit_projection`] instead; they do not represent a durable
    /// recovery boundary.
    pub fn observe_recovery_trace(
        &self,
        trace_id: TraceId,
    ) -> Option<andromeda_observe::RecoveryTrace> {
        let acceptance = self.acceptance()?;
        let corruption_boundary_lsn = match acceptance.observed_boundary {
            ObservedBoundary::Clean => None,
            ObservedBoundary::RecoverableTail | ObservedBoundary::ForensicChainBreak => {
                Some(self.evidence.last_durable_lsn.get())
            },
        };
        Some(andromeda_observe::RecoveryTrace {
            trace_id,
            last_durable_lsn: self.evidence.last_durable_lsn.get(),
            corruption_boundary_lsn,
        })
    }
}

/// Auditable projection of a startup decision. Carries mode + durable
/// evidence + outcome; safe to serialize into a critical-decision event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupAuditProjection {
    pub trace_id: TraceId,
    pub mode: StartupMode,
    pub mounted_snapshot_id: u64,
    pub required_wal_start_lsn: crate::Lsn,
    pub last_durable_lsn: crate::Lsn,
    pub observed_boundary: ObservedBoundary,
    pub forensic_report_attached: bool,
    pub outcome: StartupOutcome,
}
