use andromeda_core::AndromedaResult;

use super::{BufferFrame, BufferFrameId, BufferFrameState, BufferPoolError};

/// Deterministic Clock eviction policy over transient buffer-pool frames.
///
/// The policy owns only the transient scan cursor. Durable page identity and
/// layout remain in the resident [`BufferFrame`] image, while pin, dirty,
/// lifecycle state, and Clock usage are interpreted as eviction metadata.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ClockEvictionPolicy {
    cursor: usize,
}

/// Stable victim descriptor for future buffer-pool integration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockEvictionCandidate {
    frame_index: usize,
    frame_id: BufferFrameId,
}

impl ClockEvictionPolicy {
    pub const fn new() -> Self {
        Self { cursor: 0 }
    }

    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Select a clean, unpinned, resident frame using Clock second-chance rules.
    ///
    /// Pinned frames are never evicted. Dirty, free, flushing, and already
    /// evicting frames are ineligible under the current [`BufferFrame`] contract:
    /// [`BufferFrame::begin_eviction`] accepts only clean unpinned resident
    /// frames. Clean unpinned frames with a set Clock usage bit receive exactly
    /// one second chance per scan and have that bit cleared deterministically.
    pub fn select_victim(
        &mut self,
        frames: &mut [BufferFrame],
    ) -> AndromedaResult<ClockEvictionCandidate> {
        if frames.is_empty() {
            return Err(BufferPoolError::NoEvictionFrames.into_andromeda_error());
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
                frame_id: frame.id(),
            });
        }

        Err(classify_exhaustion(frames).into_andromeda_error())
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

fn is_clean_unpinned_resident(frame: &BufferFrame) -> bool {
    frame.state() == BufferFrameState::Resident && frame.pin_count() == 0 && !frame.is_dirty()
}

fn classify_exhaustion(frames: &[BufferFrame]) -> BufferPoolError {
    if frames
        .iter()
        .all(|frame| frame.state() == BufferFrameState::Resident && frame.pin_count() > 0)
    {
        BufferPoolError::AllFramesPinned
    } else {
        BufferPoolError::NoEvictableFrame
    }
}

#[cfg(test)]
mod tests {
    use andromeda_core::AndromedaErrorKind;

    use super::*;
    use crate::{
        AllocationId, Lsn, ObjectId, PageFlags, PageHeader, PageId, PageLayoutContract, PageSize,
        PageTrailer, PageType,
    };

    fn frame(frame_id: usize, page_id: u64) -> BufferFrame {
        BufferFrame::with_layout(
            BufferFrameId::new(frame_id).expect("valid frame id"),
            valid_contract(PageId::new(page_id), PageSize::KiB16, Lsn::new(10)),
        )
        .expect("resident frame")
    }

    fn valid_contract(page_id: PageId, page_size: PageSize, page_lsn: Lsn) -> PageLayoutContract {
        PageLayoutContract {
            header: PageHeader {
                magic: PageHeader::MAGIC,
                format_version: PageHeader::FORMAT_VERSION_V0,
                page_size,
                page_type: PageType::FixedRow,
                page_id,
                object_id: ObjectId::new(2),
                allocation_id: AllocationId::new(3),
                page_lsn,
                page_epoch: 1,
                previous_page_id: None,
                next_page_id: None,
                header_len: PageHeader::MIN_HEADER_LEN_V0,
                payload_offset: 128,
                payload_len: 512,
                free_start: 256,
                free_end: 512,
                free_bytes: 256,
                slot_count: 1,
                row_count: 1,
                flags: PageFlags::NONE,
                header_crc: 5,
            },
            trailer: PageTrailer {
                payload_crc64: 6,
                page_hash: [7; 32],
                torn_write_guard: 8,
            },
        }
    }

    #[test]
    fn selects_clean_unpinned_resident_victim() {
        let mut frames = vec![frame(1, 101)];
        assert!(frames[0].consume_clock_usage().expect("clear usage"));

        let mut clock = ClockEvictionPolicy::new();
        let victim = clock.select_victim(&mut frames).expect("victim");

        assert_eq!(victim.frame_index(), 0);
        assert_eq!(victim.frame_id(), BufferFrameId::new(1).expect("frame id"));
        assert_eq!(clock.cursor(), 0);
    }

    #[test]
    fn clears_usage_bit_and_gives_second_chance() {
        let mut frames = vec![frame(1, 101), frame(2, 102)];
        assert!(frames[1].consume_clock_usage().expect("clear second usage"));

        let mut clock = ClockEvictionPolicy::new();
        let victim = clock.select_victim(&mut frames).expect("victim");

        assert_eq!(victim.frame_index(), 1);
        assert_eq!(victim.frame_id(), BufferFrameId::new(2).expect("frame id"));
        assert!(!frames[0].clock_usage());
        assert_eq!(clock.cursor(), 0);

        let second_victim = clock
            .select_victim(&mut frames)
            .expect("second-chance victim on next pass");
        assert_eq!(second_victim.frame_index(), 0);
    }

    #[test]
    fn rejects_all_pinned_frames_explicitly() {
        let mut frames = vec![frame(1, 101), frame(2, 102)];
        frames[0].pin().expect("pin first");
        frames[1].pin().expect("pin second");

        let mut clock = ClockEvictionPolicy::new();
        let error = clock
            .select_victim(&mut frames)
            .expect_err("all pinned frames are exhausted");

        assert_eq!(error.kind(), AndromedaErrorKind::Storage);
        assert!(error.message().contains("every resident frame is pinned"));
    }

    #[test]
    fn rejects_dirty_only_no_eligible_frames_explicitly() {
        let mut frames = vec![frame(1, 101), frame(2, 102)];
        frames[0].pin().expect("pin first for dirty mutation");
        frames[0].mark_dirty(Lsn::new(10)).expect("dirty first");
        frames[0].unpin().expect("unpin first dirty frame");
        frames[1].pin().expect("pin second for dirty mutation");
        frames[1].mark_dirty(Lsn::new(10)).expect("dirty second");
        frames[1].unpin().expect("unpin second dirty frame");

        let mut clock = ClockEvictionPolicy::new();
        let error = clock
            .select_victim(&mut frames)
            .expect_err("dirty-only frames are ineligible");

        assert_eq!(error.kind(), AndromedaErrorKind::Storage);
        assert!(error.message().contains("no clean unpinned resident frame"));
        assert!(frames[0].clock_usage());
        assert!(frames[1].clock_usage());
    }

    #[test]
    fn handles_empty_frame_list_explicitly() {
        let mut frames = Vec::new();
        let mut clock = ClockEvictionPolicy::new();

        let error = clock
            .select_victim(&mut frames)
            .expect_err("empty frame list is exhausted");

        assert_eq!(error.kind(), AndromedaErrorKind::Storage);
        assert!(error.message().contains("at least one buffer frame"));
    }

    #[test]
    fn cursor_progression_is_deterministic() {
        let mut frames = vec![frame(1, 101), frame(2, 102), frame(3, 103)];
        for frame in &mut frames {
            assert!(frame.consume_clock_usage().expect("clear usage"));
        }
        let mut clock = ClockEvictionPolicy::new();

        let first = clock.select_victim(&mut frames).expect("first victim");
        let second = clock.select_victim(&mut frames).expect("second victim");
        let third = clock.select_victim(&mut frames).expect("third victim");
        let fourth = clock.select_victim(&mut frames).expect("wrapped victim");

        assert_eq!(first.frame_id(), BufferFrameId::new(1).expect("frame id"));
        assert_eq!(second.frame_id(), BufferFrameId::new(2).expect("frame id"));
        assert_eq!(third.frame_id(), BufferFrameId::new(3).expect("frame id"));
        assert_eq!(fourth.frame_id(), BufferFrameId::new(1).expect("frame id"));
        assert_eq!(clock.cursor(), 1);
    }
}
