use andromeda_core::{AndromedaError, AndromedaErrorKind};

/// Typed validation failures for transient buffer-pool metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BufferPoolError {
    InvalidFrameCount {
        frame_count: usize,
        max_frame_count: usize,
    },
    InvalidFrameId {
        frame_id: usize,
    },
    InvalidPageId,
    InvalidDirtyLsn,
    InvalidPinCount,
    InvalidPageLayout,
    InvalidPageImage,
    InvalidFrameState,
    InvalidClockUsage,
    NoEvictionFrames,
    AllFramesPinned,
    NoEvictableFrame,
    PageNotFound {
        page_id: u64,
    },
    PageSizeMismatch,
    WalDurabilityRequired,
    PageTableConflict {
        page_id: u64,
    },
}

impl BufferPoolError {
    pub fn into_andromeda_error(self) -> AndromedaError {
        AndromedaError::new(AndromedaErrorKind::Storage, self.message())
    }

    fn message(&self) -> String {
        match self {
            Self::InvalidFrameCount {
                frame_count,
                max_frame_count,
            } => format!("buffer pool frame count {frame_count} must be in 1..={max_frame_count}"),
            Self::InvalidFrameId { frame_id } => {
                format!("buffer frame id {frame_id} must not be zero")
            },
            Self::InvalidPageId => "buffer frame page id must not be zero".to_string(),
            Self::InvalidDirtyLsn => "dirty buffer frame LSN must not be zero".to_string(),
            Self::InvalidPinCount => "buffer frame pin count is invalid".to_string(),
            Self::InvalidPageLayout => "buffer frame page layout contract is invalid".to_string(),
            Self::InvalidPageImage => {
                "buffer frame page image does not match resident metadata".to_string()
            },
            Self::InvalidFrameState => {
                "buffer frame lifecycle state transition is invalid".to_string()
            },
            Self::InvalidClockUsage => {
                "free buffer frame must not have Clock usage set".to_string()
            },
            Self::NoEvictionFrames => {
                "Clock eviction requires at least one buffer frame".to_string()
            },
            Self::AllFramesPinned => {
                "Clock eviction exhausted all frames because every resident frame is pinned"
                    .to_string()
            },
            Self::NoEvictableFrame => {
                "Clock eviction found no clean unpinned resident frame".to_string()
            },
            Self::PageNotFound { page_id } => {
                format!("page {page_id} is not present in the page store")
            },
            Self::PageSizeMismatch => {
                "buffer pool page size does not match the page store or image".to_string()
            },
            Self::WalDurabilityRequired => {
                "WAL durability observer is required before flushing dirty buffer pages".to_string()
            },
            Self::PageTableConflict { page_id } => {
                format!("buffer pool frame table already contains resident page {page_id}")
            },
        }
    }
}

impl From<andromeda_buffer_pool::BufferPoolCoreError> for BufferPoolError {
    fn from(value: andromeda_buffer_pool::BufferPoolCoreError) -> Self {
        match value {
            andromeda_buffer_pool::BufferPoolCoreError::InvalidFrameCount {
                frame_count,
                max_frame_count,
            } => Self::InvalidFrameCount {
                frame_count,
                max_frame_count,
            },
            andromeda_buffer_pool::BufferPoolCoreError::InvalidFrameId { frame_id } => {
                Self::InvalidFrameId { frame_id }
            },
            andromeda_buffer_pool::BufferPoolCoreError::InvalidPageId => Self::InvalidPageId,
            andromeda_buffer_pool::BufferPoolCoreError::InvalidDirtyLsn => Self::InvalidDirtyLsn,
            andromeda_buffer_pool::BufferPoolCoreError::InvalidPinCount => Self::InvalidPinCount,
            andromeda_buffer_pool::BufferPoolCoreError::InvalidFrameState => {
                Self::InvalidFrameState
            },
            andromeda_buffer_pool::BufferPoolCoreError::InvalidClockUsage => {
                Self::InvalidClockUsage
            },
            andromeda_buffer_pool::BufferPoolCoreError::NoEvictionFrames => Self::NoEvictionFrames,
            andromeda_buffer_pool::BufferPoolCoreError::AllFramesPinned => Self::AllFramesPinned,
            andromeda_buffer_pool::BufferPoolCoreError::NoEvictableFrame => Self::NoEvictableFrame,
            andromeda_buffer_pool::BufferPoolCoreError::PageNotFound { page_id } => {
                Self::PageNotFound { page_id }
            },
            andromeda_buffer_pool::BufferPoolCoreError::PageSizeMismatch => Self::PageSizeMismatch,
            andromeda_buffer_pool::BufferPoolCoreError::WalDurabilityRequired => {
                Self::WalDurabilityRequired
            },
            andromeda_buffer_pool::BufferPoolCoreError::PageTableConflict { page_id } => {
                Self::PageTableConflict { page_id }
            },
            andromeda_buffer_pool::BufferPoolCoreError::InvalidPageLayout => {
                Self::InvalidPageLayout
            },
            andromeda_buffer_pool::BufferPoolCoreError::InvalidPageImage => Self::InvalidPageImage,
        }
    }
}

pub(crate) fn map_core_error(error: andromeda_buffer_pool::BufferPoolCoreError) -> AndromedaError {
    BufferPoolError::from(error).into_andromeda_error()
}
