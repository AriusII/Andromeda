use crate::error::BufferPoolCoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferPoolFrameConfig {
    frame_count: usize,
}

impl BufferPoolFrameConfig {
    pub const DEFAULT_FRAME_COUNT: usize = 1024;
    pub const MAX_FRAME_COUNT: usize = 1_048_576;
    pub const DEFAULT: Self = Self {
        frame_count: Self::DEFAULT_FRAME_COUNT,
    };

    pub fn new(frame_count: usize) -> Result<Self, BufferPoolCoreError> {
        let config = Self { frame_count };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), BufferPoolCoreError> {
        if self.frame_count == 0 || self.frame_count > Self::MAX_FRAME_COUNT {
            return Err(BufferPoolCoreError::InvalidFrameCount {
                frame_count: self.frame_count,
                max_frame_count: Self::MAX_FRAME_COUNT,
            });
        }
        Ok(())
    }

    pub const fn frame_count(self) -> usize {
        self.frame_count
    }
}

impl Default for BufferPoolFrameConfig {
    fn default() -> Self {
        Self {
            frame_count: Self::DEFAULT_FRAME_COUNT,
        }
    }
}
