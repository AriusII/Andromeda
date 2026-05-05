use andromeda_core::AndromedaResult;

use crate::PageSize;

use super::BufferPoolError;

/// Transient buffer-pool sizing policy.
///
/// `page_size` deliberately uses the canonical durable [`PageSize`] contract;
/// the buffer pool does not define page-layout sizes of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferPoolConfig {
    frame_count: usize,
    page_size: PageSize,
}

impl BufferPoolConfig {
    pub const DEFAULT_FRAME_COUNT: usize = 1024;
    pub const MAX_FRAME_COUNT: usize = 1_048_576;
    pub const DEFAULT_PAGE_SIZE: PageSize = PageSize::KiB16;

    pub fn new(frame_count: usize, page_size: PageSize) -> AndromedaResult<Self> {
        let config = Self {
            frame_count,
            page_size,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.frame_count == 0 {
            return Err(BufferPoolError::InvalidFrameCount {
                frame_count: self.frame_count,
                max_frame_count: Self::MAX_FRAME_COUNT,
            }
            .into_andromeda_error());
        }
        if self.frame_count > Self::MAX_FRAME_COUNT {
            return Err(BufferPoolError::InvalidFrameCount {
                frame_count: self.frame_count,
                max_frame_count: Self::MAX_FRAME_COUNT,
            }
            .into_andromeda_error());
        }
        Ok(())
    }

    pub const fn frame_count(self) -> usize {
        self.frame_count
    }

    pub const fn page_size(self) -> PageSize {
        self.page_size
    }
}

impl Default for BufferPoolConfig {
    fn default() -> Self {
        Self {
            frame_count: Self::DEFAULT_FRAME_COUNT,
            page_size: Self::DEFAULT_PAGE_SIZE,
        }
    }
}
