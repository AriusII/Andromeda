use crate::{DatabaseManifest, Lsn, WalScanResult, WalScanStop, WalScanStopReason};

/// Inputs collected from durable sources before a startup decision is taken.
///
/// All fields must be derived from the validated [`crate::DatabaseManifest`]
/// and the durable WAL byte stream. Populating any field from RAM-only state
/// is a doctrine violation and will be classified as
/// [`StartupRejectionReason::RamOnlyEvidence`](super::StartupRejectionReason::RamOnlyEvidence)
/// by [`decide_startup`](super::decide_startup).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupEvidence {
    /// `true` iff the manifest has been validated against the cold-snapshot
    /// invariants (see [`crate::DatabaseManifest::validate`]).
    pub manifest_validated: bool,
    /// Identifier of the cold snapshot the recovery executor will mount.
    /// `0` means "no cold snapshot present" and is always rejected.
    pub mounted_snapshot_id: u64,
    /// The LSN at which the manifest requires WAL replay to start
    /// (`required_wal_start_lsn`). This anchors the durable replay range.
    pub required_wal_start_lsn: Lsn,
    /// Highest LSN that survived the durable WAL scan. Must be derived from
    /// bytes that were `fsync`'d to the WAL device. RAM-only appends MUST
    /// NOT be reflected here.
    pub last_durable_lsn: Lsn,
    /// The optional stop record returned by the durable WAL scan. `None`
    /// means a clean tail; otherwise the reason classifies the boundary.
    pub wal_scan_stop: Option<WalScanStop>,
    /// `true` iff a forensic report has been produced and persisted (e.g.
    /// [`crate::FileWalRecoveryReportV0`]). Forensic startup is rejected
    /// without it.
    pub forensic_report_attached: bool,
}

impl StartupEvidence {
    /// Build startup evidence from durable manifest + WAL scan inputs.
    ///
    /// This helper deliberately records manifest validation as evidence instead
    /// of returning an error. The caller can therefore pass the result to
    /// [`decide_startup`](super::decide_startup) and obtain an auditable
    /// rejection when the manifest is invalid, rather than losing the startup
    /// attempt behind an early I/O style error.
    pub fn from_manifest_and_wal_scan(
        manifest: &DatabaseManifest,
        scan: &WalScanResult,
        forensic_report_attached: bool,
    ) -> Self {
        Self {
            manifest_validated: manifest.validate().is_ok(),
            mounted_snapshot_id: manifest.snapshot_id,
            required_wal_start_lsn: manifest.required_wal_start_lsn,
            last_durable_lsn: scan.last_valid_lsn.unwrap_or(Lsn::ZERO),
            wal_scan_stop: scan.stopped,
            forensic_report_attached,
        }
    }

    /// Classify the durable WAL boundary observed by the scan.
    pub const fn observed_boundary(&self) -> ObservedBoundary {
        match self.wal_scan_stop {
            None => ObservedBoundary::Clean,
            Some(stop) => match stop.reason {
                WalScanStopReason::LsnGap
                | WalScanStopReason::DuplicateOrReorderedLsn
                | WalScanStopReason::PreviousLsnMismatch => ObservedBoundary::ForensicChainBreak,
                _ => ObservedBoundary::RecoverableTail,
            },
        }
    }

    /// Returns `true` iff the durable WAL prefix actually covers the
    /// manifest's required start LSN. When `false`, the decision must be
    /// rejected: no RAM state may stand in for the missing durable bytes.
    pub const fn durable_wal_covers_anchor(&self) -> bool {
        self.last_durable_lsn.get() >= self.required_wal_start_lsn.get()
            && self.required_wal_start_lsn.get() != 0
    }
}

/// Classification of the durable-WAL tail boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedBoundary {
    /// The WAL scan reached the end of durable bytes with no anomaly.
    Clean,
    /// The WAL scan stopped on a recoverable suffix issue (truncated or
    /// corrupt trailing record). Truth before that boundary is intact.
    RecoverableTail,
    /// The WAL scan observed a chain break inside the durable prefix
    /// (LSN gap, duplicate / reordered LSN, or previous-LSN mismatch).
    /// This requires forensic intervention; non-forensic modes must
    /// refuse to start.
    ForensicChainBreak,
}

impl ObservedBoundary {
    pub const fn requires_forensic_handling(self) -> bool {
        matches!(self, Self::ForensicChainBreak)
    }
}
