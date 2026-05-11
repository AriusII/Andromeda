pub(super) fn restore_error(message: impl Into<String>) -> crate::RestoreValidationError {
    crate::RestoreValidationError::new(message)
}

pub(super) fn map_backup_validation<T>(
    result: andromeda_backup::BackupResult<T>,
) -> crate::RestoreResult<T> {
    result.map_err(|error| restore_error(error.message()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestorePlanError {
    ExplicitPitrTargetRequired,
    ManifestChecksumMissing,
    PreflightChecksumMissing,
    PreflightChecksumMismatch,
    PlanCodecMagicMismatch,
    PlanCodecUnsupportedVersion {
        version: u16,
    },
    PlanCodecTruncated,
    PlanCodecTrailingBytes,
    PlanCodecInvalidEnum {
        field: &'static str,
        value: u8,
    },
    PlanCodecValueInvalid {
        field: &'static str,
        message: &'static str,
    },
    PlanIdentityMismatch,
    UnexpectedReplayForSnapshotTarget,
    MissingReplayCoverage,
    ReplaySegmentOrderInvalid,
    ReplaySegmentChainInvalid,
    ReplayStopMismatch,
    CompletionReplayMismatch,
    OrchestrationInvalid {
        message: String,
    },
}

impl RestorePlanError {
    pub(crate) fn orchestration_invalid(error: &crate::RestoreValidationError) -> Self {
        Self::OrchestrationInvalid {
            message: error.message().to_string(),
        }
    }

    pub const fn as_str(&self) -> &str {
        match self {
            Self::ExplicitPitrTargetRequired => "restore plan requires an explicit PITR target LSN",
            Self::ManifestChecksumMissing => "restore plan manifest checksum must not be zero",
            Self::PreflightChecksumMissing => "restore plan preflight checksum must not be zero",
            Self::PreflightChecksumMismatch => {
                "restore completion evidence checksum must match restore plan preflight checksum"
            },
            Self::PlanCodecMagicMismatch => "restore plan codec magic mismatch",
            Self::PlanCodecUnsupportedVersion { .. } => {
                "restore plan codec version is not supported"
            },
            Self::PlanCodecTruncated => "restore plan codec payload is truncated",
            Self::PlanCodecTrailingBytes => "restore plan codec payload contains trailing bytes",
            Self::PlanCodecInvalidEnum { .. } => {
                "restore plan codec payload contains invalid enum discriminant"
            },
            Self::PlanCodecValueInvalid { .. } => {
                "restore plan codec payload contains invalid field value"
            },
            Self::PlanIdentityMismatch => {
                "restore plan identity does not match codec payload contents"
            },
            Self::UnexpectedReplayForSnapshotTarget => {
                "snapshot-only PITR target must not include WAL replay segments"
            },
            Self::MissingReplayCoverage => "replay segments do not cover the PITR target LSN",
            Self::ReplaySegmentOrderInvalid => {
                "replay segment plan summary must be ordered and contiguous by sequence index"
            },
            Self::ReplaySegmentChainInvalid => {
                "replay segment plan summary must be contiguous by LSN range"
            },
            Self::ReplayStopMismatch => "replay stop LSN must match the explicit PITR target LSN",
            Self::CompletionReplayMismatch => {
                "restore completion replayed LSN must match restore plan replay stop"
            },
            Self::OrchestrationInvalid { message } => message.as_str(),
        }
    }
}

impl std::fmt::Display for RestorePlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlanCodecUnsupportedVersion { version } => {
                write!(f, "{}: {}", self.as_str(), version)
            },
            Self::PlanCodecInvalidEnum { field, value } => {
                write!(f, "{}: {}={}", self.as_str(), field, value)
            },
            Self::PlanCodecValueInvalid { field, message } => {
                write!(f, "{}: {} {}", self.as_str(), field, message)
            },
            _ => f.write_str(self.as_str()),
        }
    }
}

impl std::error::Error for RestorePlanError {}
