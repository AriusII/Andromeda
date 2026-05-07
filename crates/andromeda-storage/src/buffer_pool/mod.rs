//! Buffer-pool residency skeleton.
//!
//! This module owns transient buffer-pool metadata only. Durable page identity,
//! layout, sizing, and recovery ordering are imported from the storage crate's
//! canonical owners in accordance with DEC-032.
//!
//! ## Overview
//!
//! The buffer pool manages the transient in-memory residency of pages fetched from durable
//! storage. It provides:
//!
//! - **Frame Management**: Fixed capacity pool of `BufferFrame` instances
//! - **Eviction Policy**: Clock-based (`ClockEvictionPolicy`) page replacement
//! - **Dirty Tracking**: Records which pages have been modified since last flush
//! - **Pin/Unpin Lifecycle**: Prevents eviction of pages actively in use
//! - **WAL Integration**: Enforces WAL-before-page-flush ordering via `WalDurabilityObserver`
//!
//! ## Architecture
//!
//! - **BufferPoolConfig**: Static configuration (frame count, page size)
//! - **`BufferPool<S>`**: Generic pool implementation over a `PageStore`
//! - **BufferPoolManager**: Trait for pluggable pool implementations
//! - **BufferFrame**: Transient metadata (page ID, pin count, dirty LSN, state)
//! - **ClockEvictionPolicy**: Circular buffer with reference bits for eviction
//! - **DirtyTracker**: Maintains set of dirty pages with dirty LSN range per page
//! - **PageGuard**/**PageGuardMut**: RAII guards for pinned page access
//!
//! ## Contracts
//!
//! All frames must:
//! - Use canonical `PageId`, `PageSize`, and `Lsn` types
//! - Validate `PageLayoutContract` before resident admission
//! - Maintain `BufferFrameState` invariants (Free → Resident → Pinned)
//! - Preserve WAL-before-page-flush ordering
//! - Never leak private implementation details
//!
//! ## Usage Example
//!
//! ```ignore
//! use andromeda_storage::{
//!     BufferPoolConfig, BufferPool, BufferPoolManager, PageSize, PageId, Lsn,
//! };
//!
//! // Create a buffer pool with 128 frames of 16 KiB pages
//! let config = BufferPoolConfig::new(128, PageSize::KiB16)?;
//!
//! // Pin a page (would normally come from page store)
//! let frame_id = buffer_pool.pin_page(PageId::new(1))?;
//!
//! // Access the page (through a guard)
//! let guard = buffer_pool.page_guard(frame_id)?;
//! let page_bytes = guard.as_slice();
//! drop(guard);
//!
//! // Mark as dirty and unpin
//! buffer_pool.unpin_page(frame_id, Some(Lsn::new(42)))?;
//!
//! // Flush dirty pages to storage once WAL durability is observable
//! let observer = wal_durability_observer;
//! buffer_pool.flush_all_dirty_with_report(&observer)?;
//! ```
//!
//! ## Error Handling
//!
//! Operations return `AndromedaResult<T>`:
//! - `BufferPoolError::FrameNotResident`: Page not in buffer pool
//! - `BufferPoolError::FrameAlreadyPinned`: Cannot evict pinned frame
//! - `BufferPoolError::PageSizeMismatch`: Page size doesn't match config
//! - `BufferPoolError::InvalidPageId`: Page ID violates invariants
//! - General storage errors via `AndromedaError`
//!
//! ## Hot/Cold Agnostic
//!
//! This module is independent of HotStore/ColdStore placement decisions.
//! Page placement is determined by the `PageStore` implementation, not the buffer pool.

mod clock;
mod config;
mod dirty;
mod error;
mod flush_result;
mod frame;
mod guard;
mod manager;
mod wal_durability;

pub use clock::{ClockEvictionCandidate, ClockEvictionPolicy};
pub use config::BufferPoolConfig;
pub use dirty::{DirtyEntry, DirtyFlushCandidate, DirtyTracker};
pub use error::BufferPoolError;
pub use flush_result::{FlushAllDirtyResult, FlushBlockedFrame, FlushError, FlushStorageOperation};
pub use frame::{BufferFrame, BufferFrameId, BufferFrameState};
pub use guard::{PageGuard, PageGuardMut};
pub use manager::{BufferPool, BufferPoolManager};
pub use wal_durability::{TestWalDurabilityObserver, WalDurabilityObserver};

#[cfg(test)]
mod tests {
    use andromeda_core::AndromedaErrorKind;

    use super::*;
    use crate::{
        AllocationId, Lsn, ObjectId, PageFlags, PageHeader, PageId, PageLayoutContract, PageSize,
        PageTrailer, PageType,
    };

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
    fn buffer_pool_config_validates_frame_bounds_and_page_size_contract() {
        let config = BufferPoolConfig::new(128, PageSize::KiB16).expect("valid config");
        assert_eq!(config.frame_count(), 128);
        assert_eq!(config.page_size(), PageSize::KiB16);
        assert_eq!(config.page_size().bytes(), 16 * 1024);

        let cold_config = BufferPoolConfig::new(64, PageSize::KiB32).expect("valid cold config");
        assert_eq!(cold_config.page_size().bytes(), 32 * 1024);

        assert_eq!(
            BufferPoolConfig::new(0, PageSize::KiB16)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
        assert_eq!(
            BufferPoolConfig::new(BufferPoolConfig::MAX_FRAME_COUNT + 1, PageSize::KiB16)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn buffer_frame_id_rejects_zero_transient_identity() {
        assert_eq!(
            BufferFrameId::new(0).unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );

        let id = BufferFrameId::new(1).expect("nonzero transient frame id");
        assert_eq!(id.get(), 1);
        assert_eq!(id.zero_based_index(), 0);
    }

    #[test]
    fn buffer_frame_metadata_uses_canonical_page_and_lsn_types() {
        let frame_id = BufferFrameId::new(2).expect("frame id");
        let image = crate::PageImage::zeroed_with_layout(valid_contract(
            PageId::new(42),
            PageSize::KiB16,
            Lsn::new(7),
        ))
        .expect("valid page image");
        let mut frame = BufferFrame::with_image(frame_id, image).expect("valid frame metadata");

        assert_eq!(frame.id(), frame_id);
        assert_eq!(frame.state(), BufferFrameState::Resident);
        assert_eq!(frame.page_id(), Some(PageId::new(42)));
        assert_eq!(frame.page_size(), PageSize::KiB16);
        assert_eq!(frame.page_lsn(), Some(Lsn::new(7)));
        assert!(!frame.is_dirty());
        assert_eq!(frame.dirty_lsn(), None);

        frame.pin().expect("pin frame for dirty mutation");
        frame.mark_dirty(Lsn::new(7)).expect("nonzero dirty lsn");
        assert!(frame.is_dirty());
        assert_eq!(frame.dirty_lsn(), Some(Lsn::new(7)));
        assert_eq!(frame.last_dirty_lsn(), Some(Lsn::new(7)));
        assert!(frame.validate().is_ok());

        frame.unpin().expect("unpin frame");
        frame
            .mark_clean_after_flush(Lsn::new(7))
            .expect("flush clean");
        assert!(frame.validate().is_ok());

        assert_eq!(
            BufferFrame::new(frame_id, PageId::new(0), PageSize::KiB16)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
        frame.pin().expect("pin frame for invalid dirty lsn check");
        assert_eq!(
            frame.mark_dirty(Lsn::ZERO).unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
        frame.unpin().expect("unpin after invalid dirty lsn check");
    }

    #[test]
    fn buffer_frame_can_attach_valid_canonical_page_layout_contract() {
        let layout = valid_contract(PageId::new(51), PageSize::KiB16, Lsn::new(9));
        let frame = BufferFrame::with_layout(BufferFrameId::new(3).expect("frame id"), layout)
            .expect("valid layout-backed frame");

        assert_eq!(frame.page_id(), Some(layout.header.page_id));
        assert_eq!(frame.page_size(), layout.header.page_size);
        assert_eq!(frame.layout_contract(), Some(layout));
        assert!(frame.validate().is_ok());

        let invalid_layout = valid_contract(PageId::new(0), PageSize::KiB16, Lsn::new(9));
        assert_eq!(
            BufferFrame::with_layout(BufferFrameId::new(4).expect("frame id"), invalid_layout)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn buffer_frame_requires_validated_resident_page_image_metadata() {
        let raw_image =
            crate::PageImage::new(PageSize::KiB16, vec![0; PageSize::KiB16.bytes_usize()])
                .expect("raw image has correct size but no layout");
        assert_eq!(
            BufferFrame::with_image(BufferFrameId::new(8).expect("frame id"), raw_image)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );

        let zero_lsn_layout = valid_contract(PageId::new(63), PageSize::KiB16, Lsn::ZERO);
        assert_eq!(
            BufferFrame::with_layout(BufferFrameId::new(9).expect("frame id"), zero_lsn_layout)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn buffer_frame_lifecycle_rejects_underflow_and_invalid_transitions() {
        let layout = valid_contract(PageId::new(61), PageSize::KiB16, Lsn::new(10));
        let mut frame = BufferFrame::with_layout(BufferFrameId::new(5).expect("frame id"), layout)
            .expect("resident frame");

        assert_eq!(
            frame.unpin().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
        frame.pin().expect("pin resident");
        assert_eq!(frame.pin_count(), 1);
        assert_eq!(
            frame.begin_eviction().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
        frame.unpin().expect("unpin resident");

        frame.pin().expect("pin for dirty mutation");
        frame.mark_dirty(Lsn::new(10)).expect("mark dirty");
        frame.unpin().expect("unpin dirty frame");
        assert_eq!(
            frame.begin_eviction().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
        frame.begin_flush().expect("unpinned dirty frame may flush");
        assert_eq!(frame.state(), BufferFrameState::Flushing);
        assert_eq!(frame.pin().unwrap_err().kind(), AndromedaErrorKind::Storage);
        frame.finish_flush(Lsn::new(10)).expect("finish flush");
        assert_eq!(frame.state(), BufferFrameState::Resident);
        assert!(!frame.is_dirty());

        frame
            .begin_eviction()
            .expect("clean unpinned frame may evict");
        assert_eq!(frame.state(), BufferFrameState::Evicting);
        frame.finish_eviction().expect("finish eviction");
        assert_eq!(frame.state(), BufferFrameState::Free);
        assert!(frame.image().is_none());
    }

    #[test]
    fn buffer_frame_dirty_lsn_ordering_and_clock_usage_are_validated() {
        let layout = valid_contract(PageId::new(62), PageSize::KiB16, Lsn::new(20));
        let mut frame = BufferFrame::with_layout(BufferFrameId::new(6).expect("frame id"), layout)
            .expect("resident frame");

        assert!(frame.clock_usage());
        assert!(frame.consume_clock_usage().expect("consume set usage"));
        assert!(!frame.clock_usage());
        assert!(!frame.consume_clock_usage().expect("consume clear usage"));
        frame.set_clock_usage().expect("set usage");
        assert!(frame.clock_usage());

        frame.pin().expect("pin for dirty mutation");
        assert_eq!(
            frame.mark_dirty(Lsn::new(19)).unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
        frame.mark_dirty(Lsn::new(20)).expect("first dirty lsn");
        frame.mark_dirty(Lsn::new(21)).expect("later dirty lsn");
        assert_eq!(frame.first_dirty_lsn(), Some(Lsn::new(20)));
        assert_eq!(frame.last_dirty_lsn(), Some(Lsn::new(21)));
        assert_eq!(
            frame.mark_dirty(Lsn::new(19)).unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
        assert_eq!(
            frame
                .mark_clean_after_flush(Lsn::new(20))
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
        frame
            .mark_clean_after_flush(Lsn::new(21))
            .expect("flush through latest dirty lsn");
        assert_eq!(frame.first_dirty_lsn(), None);
        assert_eq!(frame.last_dirty_lsn(), None);
        frame.unpin().expect("unpin after dirty ordering checks");

        let mut free_frame =
            BufferFrame::free(BufferFrameId::new(7).expect("frame id"), PageSize::KiB16)
                .expect("free frame");
        assert_eq!(
            free_frame.set_clock_usage().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn page_guard_drop_unpins_acquired_pin_once() {
        let layout = valid_contract(PageId::new(71), PageSize::KiB16, Lsn::new(30));
        let mut frame = BufferFrame::with_layout(BufferFrameId::new(10).expect("frame id"), layout)
            .expect("resident frame");

        {
            let guard = frame.pin_guard().expect("pin immutable guard");
            assert_eq!(guard.pin_count(), 1);
            assert_eq!(guard.page_id(), Some(PageId::new(71)));
            assert!(guard.image().is_some());
        }

        assert_eq!(frame.pin_count(), 0);
        assert_eq!(
            frame.unpin().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn page_guard_explicit_unpin_disarms_drop() {
        let layout = valid_contract(PageId::new(72), PageSize::KiB16, Lsn::new(31));
        let mut frame = BufferFrame::with_layout(BufferFrameId::new(11).expect("frame id"), layout)
            .expect("resident frame");

        let guard = frame.pin_guard().expect("pin immutable guard");
        assert_eq!(guard.pin_count(), 1);
        guard.unpin().expect("explicit unpin");

        assert_eq!(frame.pin_count(), 0);
        assert!(frame.validate().is_ok());
    }

    #[test]
    fn page_guard_mut_marks_dirty_and_preserves_first_dirty_lsn() {
        let layout = valid_contract(PageId::new(73), PageSize::KiB16, Lsn::new(40));
        let mut frame = BufferFrame::with_layout(BufferFrameId::new(12).expect("frame id"), layout)
            .expect("resident frame");

        {
            let mut guard = frame.pin_guard_mut().expect("pin mutable guard");
            guard.mark_dirty(Lsn::new(40)).expect("mark first dirty");
            guard.mark_dirty(Lsn::new(41)).expect("later dirty lsn");
            assert!(guard.is_dirty());
            assert_eq!(guard.first_dirty_lsn(), Some(Lsn::new(40)));
            assert_eq!(guard.last_dirty_lsn(), Some(Lsn::new(41)));
            assert_eq!(guard.pin_count(), 1);
        }

        assert_eq!(frame.pin_count(), 0);
        assert!(frame.is_dirty());
        assert_eq!(frame.first_dirty_lsn(), Some(Lsn::new(40)));
        assert_eq!(frame.last_dirty_lsn(), Some(Lsn::new(41)));
    }

    #[test]
    fn page_guard_mut_dirty_image_access_requires_valid_dirty_lsn() {
        let layout = valid_contract(PageId::new(74), PageSize::KiB16, Lsn::new(50));
        let mut frame = BufferFrame::with_layout(BufferFrameId::new(13).expect("frame id"), layout)
            .expect("resident frame");

        {
            let mut guard = frame.pin_guard_mut().expect("pin mutable guard");
            assert_eq!(
                guard.dirty_image_mut(Lsn::new(49)).unwrap_err().kind(),
                AndromedaErrorKind::Storage
            );
            let image = guard
                .dirty_image_mut(Lsn::new(50))
                .expect("valid dirty image access")
                .expect("resident image");
            assert_eq!(image.page_id(), Some(PageId::new(74)));
            assert_eq!(guard.first_dirty_lsn(), Some(Lsn::new(50)));
        }

        assert_eq!(frame.pin_count(), 0);
        assert!(frame.is_dirty());
    }

    #[test]
    fn pinned_guard_frame_cannot_be_evicted_by_frame_lifecycle() {
        let layout = valid_contract(PageId::new(75), PageSize::KiB16, Lsn::new(60));
        let mut frame = BufferFrame::with_layout(BufferFrameId::new(14).expect("frame id"), layout)
            .expect("resident frame");

        frame.pin().expect("simulate acquired guard pin");
        assert_eq!(
            frame.begin_eviction().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
        frame.unpin().expect("release simulated guard pin");
    }
}
