use andromeda_core::AndromedaResult;

pub use crate::format_version::StorageFormatFingerprint;
use crate::format_version::{CompatibilityMatrix, FormatVersion, StorageFormatKind};
use crate::{DatabaseManifest, Lsn, StorageFormatManifest, WalRecord, WalScanResult};
pub use andromeda_recovery::{
    ConceptualRedoPlan, RecoveryManifestView, RedoRecordDecision, RedoRecordPlan, StartupMode,
};

use super::storage_error;

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
                Err(storage_error(reason.message()))
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

/// Storage compatibility adapter over `andromeda_recovery::RecoveryPlan`.
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
        let plan = andromeda_recovery::RecoveryPlan::from_manifest(manifest, startup_mode)?;
        Ok(Self::from_owner(plan))
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
        andromeda_recovery::RecoveryPlan::from_manifest_and_wal_scan(manifest, startup_mode, scan)
    }

    pub fn build_redo_plan(
        self,
        durable_records: &[WalRecord],
    ) -> AndromedaResult<ConceptualRedoPlan> {
        self.into_owner().build_redo_plan(durable_records)
    }

    fn from_owner(plan: andromeda_recovery::RecoveryPlan) -> Self {
        Self {
            startup_mode: plan.startup_mode,
            mounted_snapshot_id: plan.mounted_snapshot_id,
            redo_from_lsn: plan.redo_from_lsn,
            discard_incomplete_transactions: plan.discard_incomplete_transactions,
        }
    }

    fn into_owner(self) -> andromeda_recovery::RecoveryPlan {
        andromeda_recovery::RecoveryPlan {
            startup_mode: self.startup_mode,
            mounted_snapshot_id: self.mounted_snapshot_id,
            redo_from_lsn: self.redo_from_lsn,
            discard_incomplete_transactions: self.discard_incomplete_transactions,
        }
    }
}
