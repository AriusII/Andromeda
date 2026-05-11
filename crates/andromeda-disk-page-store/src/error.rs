use andromeda_error::{AndromedaError, AndromedaErrorKind};

/// Error types for disk manager operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiskManagerError {
    /// Page ID does not correspond to any allocated extent.
    PageNotAllocated { page_id: u64 },
    /// Page was not found on disk (unallocated region).
    PageNotFound { page_id: u64 },
    /// Page read from disk has corrupted checksum.
    PageCorrupted { page_id: u64, reason: String },
    /// Page image or persisted layout is structurally invalid.
    PageLayoutInvalid { reason: String },
    /// Invalid extent descriptor (overlapping, invalid bounds, etc.).
    InvalidExtentDescriptor { reason: String },
    /// File offset computation overflowed.
    OffsetOverflow { page_id: u64, reason: String },
    /// Extent not found in allocation table.
    ExtentNotFound { extent_id: u64 },
    /// I/O operation failed (file not found, permission, etc.).
    IoError { operation: String, reason: String },
    /// Extent allocation metadata inconsistent.
    ExtentMetadataInconsistent { reason: String },
    /// Multiple extents claim same file range.
    ExtentOverlap { reason: String },
    /// Disk space exhausted.
    DiskSpaceExhausted { reason: String },
    /// A page flush was attempted before the page LSN was durable in WAL.
    WalFenceViolation { page_lsn: u64, durable_lsn: u64 },
    /// A temp/spill write was attempted with an invalid `WalIoQueueClass`.
    ///
    /// Temp writes must never be classified as `P0Durability`.  Only the
    /// WAL→commit critical path may hold that class.  This error is returned
    /// by [`crate::FileDiskManager::atomic_write_page`] and by
    /// [`crate::FileDiskManager::with_temp_write_class`] when P0 is supplied.
    QueueClassViolation { reason: String },
}

impl std::fmt::Display for DiskManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PageNotAllocated { page_id } => write!(f, "page {} not allocated", page_id),
            Self::PageNotFound { page_id } => write!(f, "page {} not found on disk", page_id),
            Self::PageCorrupted { page_id, reason } => {
                write!(f, "page {} corrupted: {}", page_id, reason)
            },
            Self::PageLayoutInvalid { reason } => write!(f, "page layout invalid: {}", reason),
            Self::InvalidExtentDescriptor { reason } => {
                write!(f, "invalid extent descriptor: {}", reason)
            },
            Self::OffsetOverflow { page_id, reason } => {
                write!(f, "offset overflow for page {}: {}", page_id, reason)
            },
            Self::ExtentNotFound { extent_id } => write!(f, "extent {} not found", extent_id),
            Self::IoError { operation, reason } => write!(f, "{} failed: {}", operation, reason),
            Self::ExtentMetadataInconsistent { reason } => {
                write!(f, "extent metadata inconsistent: {}", reason)
            },
            Self::ExtentOverlap { reason } => write!(f, "extent overlap: {}", reason),
            Self::DiskSpaceExhausted { reason } => write!(f, "disk space exhausted: {}", reason),
            Self::WalFenceViolation {
                page_lsn,
                durable_lsn,
            } => write!(
                f,
                "WAL-before-page flush violated: page LSN {} exceeds durable WAL LSN {}",
                page_lsn, durable_lsn
            ),
            Self::QueueClassViolation { reason } => {
                write!(f, "WAL queue class violation: {}", reason)
            },
        }
    }
}

impl std::error::Error for DiskManagerError {}

impl From<DiskManagerError> for AndromedaError {
    fn from(value: DiskManagerError) -> Self {
        Self::new(AndromedaErrorKind::Storage, value.to_string())
    }
}
