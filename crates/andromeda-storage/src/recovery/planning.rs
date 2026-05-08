use andromeda_core::AndromedaResult;
use andromeda_observe::TraceId;

pub use crate::format_version::StorageFormatFingerprint;
use crate::format_version::{CompatibilityMatrix, FormatVersion, StorageFormatKind};
use crate::{
    DatabaseManifest, DurableTransactionResume, DurableTransactionState,
    IncompleteDurableTransaction, Lsn, StorageFormatManifest, WalRecord, WalRecordKind,
    WalScanResult, WalScanStop, WalScanStopReason, summarize_transactions_from_records,
};
pub use andromeda_recovery::StartupMode;

use super::{
    coverage::{WalCoverageEvidence, validate_wal_coverage},
    storage_error,
};

/// Minimal DEC-032 storage-format evidence required before recovery redo.
///
/// This is intentionally a narrow recovery-side gate: it does not claim that
/// heap/page/index manifests are fully wired yet. It gives callers that do have
/// durable fingerprints a typed place to validate them before `build_redo_plan`
/// can inspect or apply WAL records. Existing WAL frame scanning remains
/// unchanged; this gate only evaluates the storage subformats that redo would
/// mutate or depend on.
pub const RECOVERY_REQUIRED_STORAGE_FORMATS: &[StorageFormatKind] = &[
    StorageFormatKind::Page,
    StorageFormatKind::HeapPage,
    StorageFormatKind::BTreeKey,
    StorageFormatKind::BTreeNode,
    StorageFormatKind::WalPayload,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreRedoStorageFormatRejection {
    Unknown {
        kind: StorageFormatKind,
    },
    Unsupported {
        kind: StorageFormatKind,
        observed: FormatVersion,
        reader: FormatVersion,
    },
    ForensicReportRequired {
        kind: StorageFormatKind,
    },
}

impl PreRedoStorageFormatRejection {
    pub fn message(self) -> String {
        match self {
            Self::Unknown { kind } => {
                format!(
                    "unknown storage subformat `{}` must be rejected before redo",
                    kind.name()
                )
            }
            Self::Unsupported {
                kind,
                observed,
                reader,
            } => format!(
                "unsupported storage subformat `{}` version {}.{} for reader {}.{} must be rejected before redo",
                kind.name(),
                observed.major,
                observed.minor,
                reader.major,
                reader.minor
            ),
            Self::ForensicReportRequired { kind } => format!(
                "ForensicStart requires a forensic report before opening unknown or unsupported `{}` without replay",
                kind.name()
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreRedoStorageFormatDecision {
    ReplayAllowed,
    ForensicReadOnly {
        reason: PreRedoStorageFormatRejection,
    },
}

impl PreRedoStorageFormatDecision {
    pub const fn replay_allowed(self) -> bool {
        matches!(self, Self::ReplayAllowed)
    }
}

pub struct PreRedoStorageFormatGate;

impl PreRedoStorageFormatGate {
    pub fn decide(
        startup_mode: StartupMode,
        forensic_report_attached: bool,
        required_formats: &[StorageFormatKind],
        observed_formats: &[StorageFormatFingerprint],
    ) -> Result<PreRedoStorageFormatDecision, PreRedoStorageFormatRejection> {
        let rejection = first_format_rejection(required_formats, observed_formats);
        match rejection {
            None => Ok(PreRedoStorageFormatDecision::ReplayAllowed),
            Some(reason) if startup_mode == StartupMode::ForensicStart => {
                if forensic_report_attached {
                    Ok(PreRedoStorageFormatDecision::ForensicReadOnly { reason })
                } else {
                    Err(PreRedoStorageFormatRejection::ForensicReportRequired {
                        kind: reason_kind(reason),
                    })
                }
            }
            Some(reason) => Err(reason),
        }
    }

    pub fn validate_replay(
        startup_mode: StartupMode,
        required_formats: &[StorageFormatKind],
        observed_formats: &[StorageFormatFingerprint],
    ) -> AndromedaResult<()> {
        match Self::decide(startup_mode, false, required_formats, observed_formats) {
            Ok(PreRedoStorageFormatDecision::ReplayAllowed) => Ok(()),
            Ok(PreRedoStorageFormatDecision::ForensicReadOnly { reason }) | Err(reason) => {
                Err(storage_error(reason.message()))
            }
        }
    }

    pub fn validate_replay_from_manifest(
        startup_mode: StartupMode,
        required_formats: &[StorageFormatKind],
        storage_format_manifest: &StorageFormatManifest,
    ) -> AndromedaResult<()> {
        storage_format_manifest.validate()?;
        Self::validate_replay(
            startup_mode,
            required_formats,
            storage_format_manifest.fingerprints(),
        )
    }
}

fn first_format_rejection(
    required_formats: &[StorageFormatKind],
    observed_formats: &[StorageFormatFingerprint],
) -> Option<PreRedoStorageFormatRejection> {
    for required in required_formats {
        let Some(observed) = observed_formats
            .iter()
            .find(|observed| observed.kind == *required)
        else {
            return Some(PreRedoStorageFormatRejection::Unknown { kind: *required });
        };

        let reader = required.current_version();
        if CompatibilityMatrix::new(reader)
            .can_read(observed.version)
            .is_err()
        {
            return Some(PreRedoStorageFormatRejection::Unsupported {
                kind: *required,
                observed: observed.version,
                reader,
            });
        }
    }

    None
}

fn reason_kind(reason: PreRedoStorageFormatRejection) -> StorageFormatKind {
    match reason {
        PreRedoStorageFormatRejection::Unknown { kind }
        | PreRedoStorageFormatRejection::Unsupported { kind, .. }
        | PreRedoStorageFormatRejection::ForensicReportRequired { kind } => kind,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryPlan {
    pub startup_mode: StartupMode,
    pub mounted_snapshot_id: u64,
    pub redo_from_lsn: Lsn,
    pub discard_incomplete_transactions: bool,
}

impl RecoveryPlan {
    pub fn from_manifest(
        manifest: &DatabaseManifest,
        startup_mode: StartupMode,
    ) -> AndromedaResult<Self> {
        manifest.validate()?;

        Ok(Self {
            startup_mode,
            mounted_snapshot_id: manifest.snapshot_id,
            redo_from_lsn: manifest.required_wal_start_lsn,
            discard_incomplete_transactions: true,
        })
    }

    pub fn from_manifest_and_wal(
        manifest: &DatabaseManifest,
        startup_mode: StartupMode,
        durable_records: &[WalRecord],
    ) -> AndromedaResult<ConceptualRedoPlan> {
        let storage_format_manifest = manifest.storage_format_manifest()?;
        PreRedoStorageFormatGate::validate_replay_from_manifest(
            startup_mode,
            RECOVERY_REQUIRED_STORAGE_FORMATS,
            &storage_format_manifest,
        )?;
        Self::from_manifest(manifest, startup_mode)?.build_redo_plan(durable_records)
    }

    pub fn from_manifest_wal_and_storage_format_manifest(
        manifest: &DatabaseManifest,
        startup_mode: StartupMode,
        durable_records: &[WalRecord],
        storage_format_manifest: &StorageFormatManifest,
    ) -> AndromedaResult<ConceptualRedoPlan> {
        PreRedoStorageFormatGate::validate_replay_from_manifest(
            startup_mode,
            RECOVERY_REQUIRED_STORAGE_FORMATS,
            storage_format_manifest,
        )?;
        Self::from_manifest(manifest, startup_mode)?.build_redo_plan(durable_records)
    }

    pub fn from_manifest_wal_and_format_fingerprints(
        manifest: &DatabaseManifest,
        startup_mode: StartupMode,
        durable_records: &[WalRecord],
        observed_formats: &[StorageFormatFingerprint],
    ) -> AndromedaResult<ConceptualRedoPlan> {
        PreRedoStorageFormatGate::validate_replay(
            startup_mode,
            RECOVERY_REQUIRED_STORAGE_FORMATS,
            observed_formats,
        )?;
        Self::from_manifest_and_wal(manifest, startup_mode, durable_records)
    }

    pub fn from_manifest_and_wal_scan(
        manifest: &DatabaseManifest,
        startup_mode: StartupMode,
        scan: &WalScanResult,
    ) -> AndromedaResult<ConceptualRedoPlan> {
        let storage_format_manifest = manifest.storage_format_manifest()?;
        PreRedoStorageFormatGate::validate_replay_from_manifest(
            startup_mode,
            RECOVERY_REQUIRED_STORAGE_FORMATS,
            &storage_format_manifest,
        )?;

        if matches!(
            scan.stopped.map(|stop| stop.reason),
            Some(
                WalScanStopReason::LsnGap
                    | WalScanStopReason::DuplicateOrReorderedLsn
                    | WalScanStopReason::PreviousLsnMismatch
            )
        ) {
            return Err(storage_error(
                "recovery WAL scan stopped at a non-recoverable LSN chain boundary",
            ));
        }

        let mut plan =
            Self::from_manifest(manifest, startup_mode)?.build_redo_plan(&scan.records)?;
        plan.wal_scan_stop = scan.stopped;
        Ok(plan)
    }

    pub fn build_redo_plan(
        self,
        durable_records: &[WalRecord],
    ) -> AndromedaResult<ConceptualRedoPlan> {
        for record in durable_records {
            record.validate()?;
        }
        let coverage = validate_wal_coverage(self.redo_from_lsn, durable_records)?;

        let durable_lsn = durable_records
            .iter()
            .map(|record| record.header.lsn)
            .max()
            .unwrap_or_default();
        let transaction_evidence = summarize_transactions_from_records(durable_records);
        let incomplete_transactions = transaction_evidence
            .iter()
            .filter(|summary| summary.is_incomplete())
            .map(|summary| IncompleteDurableTransaction {
                transaction_id: summary.transaction_id,
                first_lsn: summary.first_lsn,
                last_lsn: summary.last_lsn,
                record_count: summary.record_count,
            })
            .collect();

        let records = durable_records
            .iter()
            .map(|record| RedoRecordPlan {
                lsn: record.header.lsn,
                kind: record.header.kind,
                transaction_id: record.header.transaction_id,
                transaction_state: record.header.transaction_id.and_then(|transaction_id| {
                    transaction_evidence
                        .iter()
                        .find(|summary| summary.transaction_id == transaction_id)
                        .map(|summary| summary.state)
                }),
                decision: redo_decision_for_record(self, record, &transaction_evidence),
            })
            .collect();

        Ok(ConceptualRedoPlan {
            startup_mode: self.startup_mode,
            mounted_snapshot_id: self.mounted_snapshot_id,
            redo_from_lsn: self.redo_from_lsn,
            durable_lsn,
            discard_incomplete_transactions: self.discard_incomplete_transactions,
            coverage,
            transaction_evidence,
            incomplete_transactions,
            records,
            wal_scan_stop: None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedoRecordDecision {
    Replay,
    SkipBeforeRedoStart,
    SkipIncompleteTransaction,
    SkipRolledBackTransaction,
    SkipMissingCommitEvidence,
    SkipNonRedoRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedoRecordPlan {
    pub lsn: Lsn,
    pub kind: WalRecordKind,
    pub transaction_id: Option<andromeda_core::TransactionId>,
    pub transaction_state: Option<DurableTransactionState>,
    pub decision: RedoRecordDecision,
}

impl RedoRecordPlan {
    pub const fn should_replay(self) -> bool {
        matches!(self.decision, RedoRecordDecision::Replay)
    }
}

/// A conceptual redo plan derived from a manifest plus the durable WAL prefix.
///
/// The plan exposes three explicit boundary slots that the recovery executor
/// must keep distinct:
///
/// 1. **Mounted cold snapshot** — `mounted_snapshot_id` identifies the
///    durable, on-disk snapshot that anchors recovery. This is the only
///    source of pre-WAL truth; RAM/hot state never participates here.
/// 2. **WAL replay range** — `redo_from_lsn` (inclusive) up to `durable_lsn`
///    (inclusive) describes which durable WAL records are eligible for
///    consideration. `coverage` proves the LSN chain is contiguous and
///    anchored to the manifest start.
/// 3. **Unavailable / corrupt WAL segment boundary** — `wal_scan_stop` is
///    `Some` when the durable WAL had a recoverable tail boundary
///    (truncated/corrupt suffix) at exactly `durable_lsn`. Forensic chain
///    breaks (gap / duplicate / previous-LSN mismatch) are rejected before
///    a plan is constructed and never surface here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConceptualRedoPlan {
    pub startup_mode: StartupMode,
    pub mounted_snapshot_id: u64,
    pub redo_from_lsn: Lsn,
    pub durable_lsn: Lsn,
    pub discard_incomplete_transactions: bool,
    pub coverage: WalCoverageEvidence,
    pub transaction_evidence: Vec<DurableTransactionResume>,
    pub incomplete_transactions: Vec<IncompleteDurableTransaction>,
    pub records: Vec<RedoRecordPlan>,
    pub wal_scan_stop: Option<WalScanStop>,
}

impl ConceptualRedoPlan {
    pub fn replay_lsns(&self) -> impl Iterator<Item = Lsn> + '_ {
        self.records
            .iter()
            .filter(|record| record.should_replay())
            .map(|record| record.lsn)
    }

    pub fn committed_redo_records(&self) -> impl Iterator<Item = &RedoRecordPlan> + '_ {
        self.records.iter().filter(|record| {
            record.should_replay()
                && match record.transaction_state {
                    Some(state) => state == DurableTransactionState::Committed,
                    None => true,
                }
        })
    }

    pub fn has_incomplete_transactions(&self) -> bool {
        !self.incomplete_transactions.is_empty()
    }

    pub fn committed_replay_lsns(&self) -> impl Iterator<Item = Lsn> + '_ {
        self.committed_redo_records().map(|record| record.lsn)
    }

    /// Highest transaction id observed in durable WAL evidence.
    ///
    /// Recovery drivers should seed the transaction-id allocator at this floor
    /// (for example via `andromeda_tx::TransactionManager::with_recovered_floor`)
    /// before serving post-restart traffic. This is not RAM truth: it is a
    /// projection of transaction ids present in the durable WAL prefix.
    pub fn recovered_transaction_id_floor(&self) -> u64 {
        self.transaction_evidence
            .iter()
            .map(|summary| summary.transaction_id.get())
            .max()
            .unwrap_or(0)
    }

    pub const fn wal_scan_stop(&self) -> Option<WalScanStop> {
        self.wal_scan_stop
    }

    /// Project the plan's durable boundary into an [`andromeda_observe::RecoveryTrace`]
    /// for emission as a `RecoveryStartup` critical decision.
    ///
    /// The trace carries:
    /// * `last_durable_lsn` — the highest LSN that survived the WAL scan and
    ///   is therefore eligible for replay (cold snapshot + durable WAL truth).
    /// * `corruption_boundary_lsn` — `Some(last_durable_lsn)` iff the WAL
    ///   scan stopped at a recoverable tail boundary (truncated / corrupt
    ///   suffix). `None` indicates a clean scan with no observed boundary.
    pub fn observe_recovery_trace(&self, trace_id: TraceId) -> andromeda_observe::RecoveryTrace {
        andromeda_observe::RecoveryTrace {
            trace_id,
            last_durable_lsn: self.durable_lsn.get(),
            corruption_boundary_lsn: self.wal_scan_stop.map(|_| self.durable_lsn.get()),
        }
    }
}

fn redo_decision_for_record(
    plan: RecoveryPlan,
    record: &WalRecord,
    transaction_evidence: &[DurableTransactionResume],
) -> RedoRecordDecision {
    if record.header.lsn < plan.redo_from_lsn {
        return RedoRecordDecision::SkipBeforeRedoStart;
    }

    if !record.header.kind.is_redo_relevant() {
        return RedoRecordDecision::SkipNonRedoRecord;
    }

    if let Some(transaction_id) = record.header.transaction_id {
        let Some(summary) = transaction_evidence
            .iter()
            .find(|summary| summary.transaction_id == transaction_id)
        else {
            return RedoRecordDecision::SkipMissingCommitEvidence;
        };

        match summary.state {
            DurableTransactionState::Committed => {}
            DurableTransactionState::RolledBack => {
                return RedoRecordDecision::SkipRolledBackTransaction;
            }
            DurableTransactionState::Open | DurableTransactionState::Incomplete => {
                if plan.discard_incomplete_transactions {
                    return RedoRecordDecision::SkipIncompleteTransaction;
                }
                return RedoRecordDecision::SkipMissingCommitEvidence;
            }
        }
    }

    RedoRecordDecision::Replay
}
