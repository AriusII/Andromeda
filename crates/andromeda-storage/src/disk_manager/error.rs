/// Error types for disk manager operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiskManagerError {
    /// Page ID does not correspond to any allocated extent.
    PageNotAllocated { page_id: u64 },
    /// Page was not found on disk (unallocated region).
    PageNotFound { page_id: u64 },
    /// Page read from disk has corrupted checksum.
    PageCorrupted { page_id: u64, reason: String },
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
}

impl std::fmt::Display for DiskManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PageNotAllocated { page_id } => write!(f, "page {} not allocated", page_id),
            Self::PageNotFound { page_id } => write!(f, "page {} not found on disk", page_id),
            Self::PageCorrupted { page_id, reason } => {
                write!(f, "page {} corrupted: {}", page_id, reason)
            }
            Self::InvalidExtentDescriptor { reason } => {
                write!(f, "invalid extent descriptor: {}", reason)
            }
            Self::OffsetOverflow { page_id, reason } => {
                write!(f, "offset overflow for page {}: {}", page_id, reason)
            }
            Self::ExtentNotFound { extent_id } => write!(f, "extent {} not found", extent_id),
            Self::IoError { operation, reason } => write!(f, "{} failed: {}", operation, reason),
            Self::ExtentMetadataInconsistent { reason } => {
                write!(f, "extent metadata inconsistent: {}", reason)
            }
            Self::ExtentOverlap { reason } => write!(f, "extent overlap: {}", reason),
            Self::DiskSpaceExhausted { reason } => write!(f, "disk space exhausted: {}", reason),
        }
    }
}

impl std::error::Error for DiskManagerError {}
