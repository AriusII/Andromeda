use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BufferPoolCoreError {
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
    InvalidPageLayout,
    InvalidPageImage,
}

impl BufferPoolCoreError {
    pub fn message(&self) -> String {
        match self {
            Self::InvalidFrameCount {
                frame_count,
                max_frame_count,
            } => format!("buffer pool frame count {frame_count} must be in 1..={max_frame_count}"),
            Self::InvalidFrameId { frame_id } => {
                format!("buffer frame id {frame_id} must not be zero")
            }
            Self::InvalidPageId => "buffer frame page id must not be zero".to_string(),
            Self::InvalidDirtyLsn => "dirty buffer frame LSN must not be zero".to_string(),
            Self::InvalidPinCount => "buffer frame pin count is invalid".to_string(),
            Self::InvalidFrameState => {
                "buffer frame lifecycle state transition is invalid".to_string()
            }
            Self::InvalidClockUsage => {
                "free buffer frame must not have Clock usage set".to_string()
            }
            Self::NoEvictionFrames => {
                "Clock eviction requires at least one buffer frame".to_string()
            }
            Self::AllFramesPinned => {
                "Clock eviction exhausted all frames because every resident frame is pinned"
                    .to_string()
            }
            Self::NoEvictableFrame => {
                "Clock eviction found no clean unpinned resident frame".to_string()
            }
            Self::PageNotFound { page_id } => {
                format!("page {page_id} is not present in the page store")
            }
            Self::PageSizeMismatch => {
                "buffer pool page size does not match the page store or image".to_string()
            }
            Self::WalDurabilityRequired => {
                "WAL durability observer is required before flushing dirty buffer pages".to_string()
            }
            Self::PageTableConflict { page_id } => {
                format!("buffer pool frame table already contains resident page {page_id}")
            }
            Self::InvalidPageLayout => "buffer frame page layout contract is invalid".to_string(),
            Self::InvalidPageImage => {
                "buffer frame page image does not match resident metadata".to_string()
            }
        }
    }
}

impl Display for BufferPoolCoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for BufferPoolCoreError {}
