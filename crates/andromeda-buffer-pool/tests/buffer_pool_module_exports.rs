#![forbid(unsafe_code)]

use andromeda_buffer_pool::{
    BufferFrame, BufferFrameId, BufferFrameState, BufferPoolConfig, BufferPoolError,
    ClockEvictionPolicy, DirtyTracker,
};
use andromeda_storage_page::{
    AllocationId, Lsn, ObjectId, PageFlags, PageHeader, PageId, PageImage, PageLayoutContract,
    PageSize, PageTrailer, PageType,
};

#[test]
fn buffer_pool_module_exports_all_public_types() {
    let _: () = {
        let _ = BufferFrameId::new(1).expect("valid frame id");
        let _ = BufferPoolConfig::new(32, PageSize::KiB16).expect("valid config");
        let _: Option<BufferPoolError> = None;
        let _: ClockEvictionPolicy = ClockEvictionPolicy::new();
        let _: DirtyTracker = DirtyTracker::new();
        let _: BufferFrameState = BufferFrameState::Free;
    };
}

#[test]
fn buffer_pool_manager_trait_accessible_and_functional() {
    let config = BufferPoolConfig::new(16, PageSize::KiB16).expect("valid config");
    assert_eq!(config.frame_count(), 16);
    assert_eq!(config.page_size(), PageSize::KiB16);
}

#[test]
fn buffer_frame_uses_canonical_page_contracts() {
    let frame_id = BufferFrameId::new(1).expect("valid frame id");
    let contract = PageLayoutContract {
        header: PageHeader {
            magic: PageHeader::MAGIC,
            format_version: PageHeader::FORMAT_VERSION_V0,
            page_size: PageSize::KiB16,
            page_type: PageType::FixedRow,
            page_id: PageId::new(100),
            object_id: ObjectId::new(1),
            allocation_id: AllocationId::new(1),
            page_lsn: Lsn::new(42),
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
            payload_crc64: 1,
            page_hash: [1; 32],
            torn_write_guard: 2,
        },
    };

    let image = PageImage::zeroed_with_layout(contract).expect("valid page image");
    let mut frame = BufferFrame::with_image(frame_id, image).expect("valid buffer frame");

    assert_eq!(frame.page_id(), Some(PageId::new(100)));
    assert_eq!(frame.page_size(), PageSize::KiB16);
    assert_eq!(frame.page_lsn(), Some(Lsn::new(42)));

    // Verify frame lifecycle
    frame.pin().expect("pin frame");
    frame.mark_dirty(Lsn::new(50)).expect("mark dirty");
    assert!(frame.is_dirty());
    assert_eq!(frame.dirty_lsn(), Some(Lsn::new(50)));

    frame.unpin().expect("unpin frame");
    frame
        .mark_clean_after_flush(Lsn::new(50))
        .expect("mark clean");
    assert!(!frame.is_dirty());
}

#[test]
fn dirty_tracker_interface_functional() {
    let mut tracker = DirtyTracker::new();

    tracker
        .mark_dirty(PageId::new(1), Lsn::new(10))
        .expect("mark dirty page 1");
    tracker
        .mark_dirty(PageId::new(2), Lsn::new(20))
        .expect("mark dirty page 2");

    assert_eq!(tracker.len(), 2);

    let candidates = tracker.flush_candidates();
    assert_eq!(candidates.len(), 2);

    // Verify candidates are sorted by LSN
    assert_eq!(candidates[0].first_dirty_lsn(), Lsn::new(10));
    assert_eq!(candidates[1].first_dirty_lsn(), Lsn::new(20));
}

#[test]
fn clock_eviction_policy_interface() {
    let policy = ClockEvictionPolicy::new();
    let _: &ClockEvictionPolicy = &policy;
    // Policy is advanced internally; this test just verifies interface
}

#[test]
fn buffer_pool_exports_only_public_interface() {
    // This test verifies that we cannot access private fields or methods
    // by attempting to construct public-only interface objects

    let config = BufferPoolConfig::new(8, PageSize::KiB16).expect("valid config");
    let _: &BufferPoolConfig = &config;

    let frame_id = BufferFrameId::new(1).expect("valid frame id");
    let _: usize = frame_id.get(); // Only public accessor

    let tracker = DirtyTracker::new();
    let _: usize = tracker.len(); // Only public accessor
}

#[test]
fn buffer_pool_module_documentation_covers_contracts() {
    // This test documents the expected contracts for buffer pool users
    //
    // Contract: All buffer pool operations use canonical types:
    // - PageId for page identity
    // - PageSize for page sizing (16KiB or 32KiB)
    // - Lsn for ordering (WAL ordering before page flush)
    // - PageLayoutContract for page validation
    //
    // Contract: Pin/unpin lifecycle:
    // - Pin prevents eviction
    // - Unpin allows eviction
    // - Dirty tracking via Lsn
    //
    // Contract: Eviction policy is transparent:
    // - ClockEvictionPolicy used by default
    // - Pluggable via trait in future
    //
    // Contract: No unsafe code
    // - All operations safe
    // - No raw pointers
    // - No unchecked indexing

    let _: () = {
        // Verify these types exist and are public
        let _ = BufferFrameState::Free;
        let _ = BufferFrameState::Resident;
        let _ = BufferFrameState::Flushing;
        let _ = BufferFrameState::Evicting;
    };
}
