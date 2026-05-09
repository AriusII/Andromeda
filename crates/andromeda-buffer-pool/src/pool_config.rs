use andromeda_error::AndromedaResult;
use andromeda_storage_page::PageSize;

use crate::pool_error::map_core_error;

/// Transient buffer-pool sizing policy.
///
/// `page_size` deliberately uses the canonical durable [`PageSize`] contract;
/// the buffer pool does not define page-layout sizes of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferPoolConfig {
    core: crate::BufferPoolFrameConfig,
    page_size: PageSize,
}

impl BufferPoolConfig {
    pub const DEFAULT_FRAME_COUNT: usize = crate::BufferPoolFrameConfig::DEFAULT_FRAME_COUNT;
    pub const MAX_FRAME_COUNT: usize = crate::BufferPoolFrameConfig::MAX_FRAME_COUNT;
    pub const DEFAULT_PAGE_SIZE: PageSize = PageSize::KiB16;

    pub fn new(frame_count: usize, page_size: PageSize) -> AndromedaResult<Self> {
        let config = Self {
            core: crate::BufferPoolFrameConfig::new(frame_count).map_err(map_core_error)?,
            page_size,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.core.validate().map_err(map_core_error)
    }

    pub const fn frame_count(self) -> usize {
        self.core.frame_count()
    }

    pub const fn page_size(self) -> PageSize {
        self.page_size
    }
}

impl Default for BufferPoolConfig {
    fn default() -> Self {
        Self {
            core: crate::BufferPoolFrameConfig::DEFAULT,
            page_size: Self::DEFAULT_PAGE_SIZE,
        }
    }
}
