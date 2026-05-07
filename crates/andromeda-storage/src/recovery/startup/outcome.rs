use super::ObservedBoundary;

/// Reasons a startup attempt may be rejected. Each variant maps to a
/// doctrine invariant and is intended to be projected into an audit event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupRejectionReason {
    /// The manifest has not been validated; cold-snapshot truth is unknown.
    ManifestNotValidated,
    /// No cold snapshot is mounted (`mounted_snapshot_id == 0`).
    MissingColdSnapshot,
    /// The durable WAL prefix does not cover the manifest's required start
    /// LSN. This includes the case where only RAM-buffered records exist.
    RamOnlyEvidence,
    /// `FastStart` was requested but the WAL scan exposed a recoverable
    /// tail boundary. FastStart cannot skip a durable-tail audit.
    FastStartRequiresCleanScan,
    /// A non-forensic mode was requested but the WAL scan exposed a chain
    /// break that requires forensic handling.
    ForensicHandlingRequired,
    /// `ForensicStart` was requested without a forensic report attached.
    ForensicStartRequiresReport,
}

impl StartupRejectionReason {
    pub const fn as_static_str(self) -> &'static str {
        match self {
            Self::ManifestNotValidated => "manifest_not_validated",
            Self::MissingColdSnapshot => "missing_cold_snapshot",
            Self::RamOnlyEvidence => "ram_only_evidence",
            Self::FastStartRequiresCleanScan => "fast_start_requires_clean_scan",
            Self::ForensicHandlingRequired => "forensic_handling_required",
            Self::ForensicStartRequiresReport => "forensic_start_requires_report",
        }
    }
}

/// Proof carried by an accepted startup decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupAcceptance {
    /// `true` iff the recovery executor is allowed to apply WAL records on
    /// top of the mounted cold snapshot under this mode.
    ///
    /// `ForensicStart` always sets this to `false`: the engine may be
    /// inspected but startup must not mutate durable truth.
    pub replay_allowed: bool,
    /// The boundary the decision was taken against.
    pub observed_boundary: ObservedBoundary,
    /// `true` iff a forensic report should be retained alongside this
    /// decision. Forensic mode always preserves the report; non-forensic
    /// modes only do so when one was supplied.
    pub forensic_report_preserved: bool,
}

/// Outcome of a startup decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupOutcome {
    Accepted(StartupAcceptance),
    Rejected(StartupRejectionReason),
}

impl StartupOutcome {
    pub const fn is_accepted(self) -> bool {
        matches!(self, Self::Accepted(_))
    }
}
