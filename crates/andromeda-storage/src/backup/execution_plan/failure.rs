/// Failure mode enumeration for physical backup execution.
///
/// These represent the categorical reasons a backup execution plan may be invalid
/// or a backup copy may fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupExecutionFailureMode {
    /// Manifest validation failed (CRC, identity, WAL range)
    ManifestInvalid,
    /// Extent descriptor is missing or mutable (not sealed/published)
    ExtentNotDurable,
    /// Extent copy plan exceeds resource limits
    ExtentResourceExhausted,
    /// WAL segment list is empty or has gaps (LSN discontinuity)
    WalSegmentChainInvalid,
    /// WAL segment missing from archive range
    WalSegmentMissing,
    /// WAL segment CRC/checksum failed validation
    WalSegmentCorrupted,
    /// WAL copy plan exceeds resource limits
    WalResourceExhausted,
    /// Destination storage full or write failed
    DestinationStorageFailure,
    /// Backup copy incomplete (partial write)
    IncompleteBackupCopy,
    /// Metadata checksum absent or invalid
    BackupMetadataInvalid,
}

impl BackupExecutionFailureMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ManifestInvalid => "backup manifest failed validation",
            Self::ExtentNotDurable => "extent copy references non-durable extent",
            Self::ExtentResourceExhausted => "extent copy plan exceeds resource limits",
            Self::WalSegmentChainInvalid => "WAL segment chain has gaps or missing segments",
            Self::WalSegmentMissing => "WAL segment missing from archive range",
            Self::WalSegmentCorrupted => "WAL segment artifact checksum validation failed",
            Self::WalResourceExhausted => "WAL copy plan exceeds resource limits",
            Self::DestinationStorageFailure => "destination storage full or write failed",
            Self::IncompleteBackupCopy => "backup copy incomplete (partial write)",
            Self::BackupMetadataInvalid => "backup metadata checksum absent or invalid",
        }
    }
}
