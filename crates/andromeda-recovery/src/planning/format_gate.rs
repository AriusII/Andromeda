use andromeda_error::AndromedaResult;
pub use andromeda_manifest::format_version::StorageFormatFingerprint;
use andromeda_manifest::{
    StorageFormatManifest,
    format_version::{CompatibilityMatrix, FormatVersion, StorageFormatKind},
};
use andromeda_wal::Lsn;

use crate::StartupMode;

use super::recovery_error;

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
