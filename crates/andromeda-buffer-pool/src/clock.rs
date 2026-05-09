use crate::error::BufferPoolCoreError;
use crate::frame::{BufferFrameCore, BufferFrameId, BufferFrameState};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ClockEvictionPolicy {
    cursor: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockEvictionCandidate {
    frame_index: usize,
    frame_id: BufferFrameId,
}

pub trait ClockFrame {
    fn clock_frame_id(&self) -> BufferFrameId;
    fn clock_state(&self) -> BufferFrameState;
    fn clock_pin_count(&self) -> u32;
    fn clock_is_dirty(&self) -> bool;
    fn consume_clock_usage(&mut self) -> Result<bool, BufferPoolCoreError>;
}

impl ClockEvictionPolicy {
    pub const fn new() -> Self {
        Self { cursor: 0 }
    }

    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn select_victim<F: ClockFrame>(
        &mut self,
        frames: &mut [F],
    ) -> Result<ClockEvictionCandidate, BufferPoolCoreError> {
        if frames.is_empty() {
            return Err(BufferPoolCoreError::NoEvictionFrames);
        }

        let frame_count = frames.len();
        self.cursor %= frame_count;

        for _ in 0..frame_count.saturating_mul(2) {
            let frame_index = self.cursor;
            self.advance(frame_count);

            let frame = &mut frames[frame_index];
            if !is_clean_unpinned_resident(frame) {
                continue;
            }

            if frame.consume_clock_usage()? {
                continue;
            }

            return Ok(ClockEvictionCandidate {
                frame_index,
                frame_id: frame.clock_frame_id(),
            });
        }

        Err(classify_exhaustion(frames))
    }

    fn advance(&mut self, frame_count: usize) {
        self.cursor = (self.cursor + 1) % frame_count;
    }
}

impl ClockEvictionCandidate {
    pub const fn frame_index(self) -> usize {
        self.frame_index
    }

    pub const fn frame_id(self) -> BufferFrameId {
        self.frame_id
    }
}

impl ClockFrame for BufferFrameCore {
    fn clock_frame_id(&self) -> BufferFrameId {
        self.id()
    }

    fn clock_state(&self) -> BufferFrameState {
        self.state()
    }

    fn clock_pin_count(&self) -> u32 {
        self.pin_count()
    }

    fn clock_is_dirty(&self) -> bool {
        self.is_dirty()
    }

    fn consume_clock_usage(&mut self) -> Result<bool, BufferPoolCoreError> {
        self.consume_clock_usage()
    }
}

fn is_clean_unpinned_resident(frame: &impl ClockFrame) -> bool {
    frame.clock_state() == BufferFrameState::Resident
        && frame.clock_pin_count() == 0
        && !frame.clock_is_dirty()
}

fn classify_exhaustion(frames: &[impl ClockFrame]) -> BufferPoolCoreError {
    if frames.iter().all(|frame| {
        frame.clock_state() == BufferFrameState::Resident && frame.clock_pin_count() > 0
    }) {
        BufferPoolCoreError::AllFramesPinned
    } else {
        BufferPoolCoreError::NoEvictableFrame
    }
}
