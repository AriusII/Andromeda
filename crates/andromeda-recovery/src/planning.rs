use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
pub use andromeda_manifest::format_version::StorageFormatFingerprint;
use andromeda_manifest::{
    StorageFormatManifest,
    format_version::{CompatibilityMatrix, FormatVersion, StorageFormatKind},
};
use andromeda_wal::{
    DurableTransactionResume, DurableTransactionState, IncompleteDurableTransaction, Lsn,
    WalRecord, WalRecordKind, WalScanResult, WalScanStop, WalScanStopReason,
    summarize_transactions_from_records,
};

use crate::{StartupMode, WalCoverageEvidence, validate_wal_coverage};

/// Durable manifest projection needed to plan recovery without depending on
/// storage's concrete manifest type.
pub trait RecoveryManifestView {
    fn validate_recovery_manifest(&self) -> AndromedaResult<()>;
    fn mounted_snapshot_id(&self) -> u64;
    fn required_wal_start_lsn(&self) -> Lsn;

    /// Optional owner-specific gate for format compatibility before redo.
    fn validate_recovery_storage_formats(&self, _startup_mode: StartupMode) -> AndromedaResult<()> {
        Ok(())
    }
}

impl RecoveryManifestView for andromeda_manifest::DatabaseManifest {
    fn validate_recovery_manifest(&self) -> AndromedaResult<()> {
        self.validate()
    }

    fn mounted_snapshot_id(&self) -> u64 {
        self.snapshot_id
    }

    fn required_wal_start_lsn(&self) -> Lsn {
        self.required_wal_start_lsn
    }

    fn validate_recovery_storage_formats(&self, startup_mode: StartupMode) -> AndromedaResult<()> {
        let storage_format_manifest = self.storage_format_manifest()?;
        PreRedoStorageFormatGate::validate_replay_from_manifest(
            startup_mode,
            RECOVERY_REQUIRED_STORAGE_FORMATS,
            &storage_format_manifest,
        )
    }
}

/// Minimal DEC-032 storage-format evidence required before recovery redo.
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
            },
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
            },
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
                Err(recovery_error(reason.message()))
            },
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
        manifest: &impl RecoveryManifestView,
        startup_mode: StartupMode,
    ) -> AndromedaResult<Self> {
        manifest.validate_recovery_manifest()?;

        Ok(Self {
            startup_mode,
            mounted_snapshot_id: manifest.mounted_snapshot_id(),
            redo_from_lsn: manifest.required_wal_start_lsn(),
            discard_incomplete_transactions: true,
        })
    }

    pub fn from_manifest_and_wal(
        manifest: &impl RecoveryManifestView,
        startup_mode: StartupMode,
        durable_records: &[WalRecord],
    ) -> AndromedaResult<ConceptualRedoPlan> {
        manifest.validate_recovery_storage_formats(startup_mode)?;
        Self::from_manifest(manifest, startup_mode)?.build_redo_plan(durable_records)
    }

    pub fn from_manifest_and_wal_scan(
        manifest: &impl RecoveryManifestView,
        startup_mode: StartupMode,
        scan: &WalScanResult,
    ) -> AndromedaResult<ConceptualRedoPlan> {
        manifest.validate_recovery_storage_formats(startup_mode)?;

        if matches!(
            scan.stopped.map(|stop| stop.reason),
            Some(
                WalScanStopReason::LsnGap
                    | WalScanStopReason::DuplicateOrReorderedLsn
                    | WalScanStopReason::PreviousLsnMismatch
            )
        ) {
            return Err(recovery_error(
                "recovery WAL scan stopped at a non-recoverable LSN chain boundary",
            ));
        }

        let mut plan =
            Self::from_manifest(manifest, startup_mode)?.build_redo_plan(&scan.records)?;
        plan.wal_scan_stop = scan.stopped;
        Ok(plan)
    }

    pub fn from_manifest_wal_and_storage_format_manifest(
        manifest: &andromeda_manifest::DatabaseManifest,
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
        manifest: &andromeda_manifest::DatabaseManifest,
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
    pub transaction_id: Option<andromeda_types::TransactionId>,
    pub transaction_state: Option<DurableTransactionState>,
    pub decision: RedoRecordDecision,
}

impl RedoRecordPlan {
    pub const fn should_replay(self) -> bool {
        matches!(self.decision, RedoRecordDecision::Replay)
    }
}

/// A conceptual redo plan derived from a manifest plus the durable WAL prefix.
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

    /// Project the plan into a trace payload without coupling recovery to the
    /// observer crate.
    pub fn trace_projection<TraceId>(&self, trace_id: TraceId) -> RecoveryTraceProjection<TraceId>
    where
        TraceId: Copy,
    {
        RecoveryTraceProjection {
            trace_id,
            last_durable_lsn: self.durable_lsn.get(),
            corruption_boundary_lsn: self.wal_scan_stop.map(|_| self.durable_lsn.get()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryTraceProjection<TraceId = u128> {
    pub trace_id: TraceId,
    pub last_durable_lsn: u64,
    pub corruption_boundary_lsn: Option<u64>,
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
            DurableTransactionState::Committed => {},
            DurableTransactionState::RolledBack => {
                return RedoRecordDecision::SkipRolledBackTransaction;
            },
            DurableTransactionState::Open | DurableTransactionState::Incomplete => {
                if plan.discard_incomplete_transactions {
                    return RedoRecordDecision::SkipIncompleteTransaction;
                }
                return RedoRecordDecision::SkipMissingCommitEvidence;
            },
        }
    }

    RedoRecordDecision::Replay
}

fn recovery_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_types::TransactionId;

    #[derive(Debug)]
    struct TestManifest {
        valid: bool,
        snapshot_id: u64,
        redo_from_lsn: Lsn,
    }

    impl RecoveryManifestView for TestManifest {
        fn validate_recovery_manifest(&self) -> AndromedaResult<()> {
            if self.valid {
                Ok(())
            } else {
                Err(recovery_error("invalid test manifest"))
            }
        }

        fn mounted_snapshot_id(&self) -> u64 {
            self.snapshot_id
        }

        fn required_wal_start_lsn(&self) -> Lsn {
            self.redo_from_lsn
        }
    }

    fn manifest() -> TestManifest {
        TestManifest {
            valid: true,
            snapshot_id: 9,
            redo_from_lsn: Lsn::new(2),
        }
    }

    fn record(
        kind: WalRecordKind,
        lsn: u64,
        previous_lsn: Option<u64>,
        tx: Option<u64>,
    ) -> WalRecord {
        WalRecord::from_parts(
            kind,
            Lsn::new(lsn),
            previous_lsn.map(Lsn::new),
            tx.map(TransactionId::new),
            Vec::new(),
        )
        .unwrap()
    }

    #[test]
    fn redo_plan_replays_committed_transaction_records() {
        let tx = 44;
        let records = vec![
            record(WalRecordKind::TxBegin, 1, None, Some(tx)),
            record(WalRecordKind::RowInsert, 2, Some(1), Some(tx)),
            record(WalRecordKind::TxCommit, 3, Some(2), Some(tx)),
        ];

        let plan =
            RecoveryPlan::from_manifest_and_wal(&manifest(), StartupMode::SafeStart, &records)
                .unwrap();

        assert_eq!(plan.mounted_snapshot_id, 9);
        assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
        assert_eq!(plan.recovered_transaction_id_floor(), tx);
        assert_eq!(plan.records[1].decision, RedoRecordDecision::Replay);
    }

    #[test]
    fn redo_plan_skips_incomplete_transaction_records() {
        let tx = 45;
        let records = vec![
            record(WalRecordKind::TxBegin, 1, None, Some(tx)),
            record(WalRecordKind::RowInsert, 2, Some(1), Some(tx)),
        ];

        let plan =
            RecoveryPlan::from_manifest_and_wal(&manifest(), StartupMode::SafeStart, &records)
                .unwrap();

        assert!(plan.has_incomplete_transactions());
        assert_eq!(
            plan.records[1].decision,
            RedoRecordDecision::SkipIncompleteTransaction
        );
    }

    #[test]
    fn redo_plan_rejects_non_recoverable_scan_boundaries() {
        let scan = WalScanResult {
            records: vec![record(WalRecordKind::PageAllocate, 2, Some(1), None)],
            valid_bytes: 0,
            last_valid_lsn: Some(Lsn::new(2)),
            stopped: Some(WalScanStop {
                reason: WalScanStopReason::LsnGap,
                offset: 16,
            }),
        };

        let err =
            RecoveryPlan::from_manifest_and_wal_scan(&manifest(), StartupMode::SafeStart, &scan)
                .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    }
}
