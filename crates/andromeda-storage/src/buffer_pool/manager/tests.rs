use andromeda_core::AndromedaErrorKind;

use super::*;
use crate::{
    AllocationId, InMemoryPageStore, ObjectId, PageFlags, PageHeader, PageSize, PageTrailer,
    PageType,
};

fn valid_contract(page_id: PageId, page_lsn: Lsn) -> PageLayoutContract {
    PageLayoutContract {
        header: PageHeader {
            magic: PageHeader::MAGIC,
            format_version: PageHeader::FORMAT_VERSION_V0,
            page_size: PageSize::KiB16,
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

fn pool(frame_count: usize) -> BufferPool<InMemoryPageStore> {
    BufferPool::new(
        BufferPoolConfig::new(frame_count, PageSize::KiB16).expect("valid config"),
        InMemoryPageStore::new(PageSize::KiB16),
    )
    .expect("valid pool")
}

#[test]
fn fetch_existing_page_loads_and_pins() {
    let mut store = InMemoryPageStore::new(PageSize::KiB16);
    store
        .allocate_page(valid_contract(PageId::new(10), Lsn::new(10)), Lsn::new(10))
        .expect("allocated page");
    let mut pool = BufferPool::new(
        BufferPoolConfig::new(2, PageSize::KiB16).expect("config"),
        store,
    )
    .expect("pool");

    let frame_id = {
        let guard = pool.fetch_page(PageId::new(10)).expect("fetch resident");
        assert_eq!(guard.page_id(), Some(PageId::new(10)));
        assert_eq!(guard.pin_count(), 1);
        guard.frame_id()
    };

    assert_eq!(pool.pin_count(frame_id).expect("pin count"), 0);
    assert_eq!(pool.resident_frame_id(PageId::new(10)), Some(frame_id));
}

#[test]
fn repeated_fetch_hits_same_frame_and_guard_drop_unpins() {
    let mut pool = pool(1);
    {
        let (_page_id, guard) = pool
            .new_page(valid_contract(PageId::new(11), Lsn::new(11)))
            .expect("new page");
        assert_eq!(guard.pin_count(), 1);
    }

    let first_frame = pool
        .resident_frame_id(PageId::new(11))
        .expect("resident frame");
    {
        let guard = pool.fetch_page(PageId::new(11)).expect("fetch hit");
        assert_eq!(guard.frame_id(), first_frame);
        assert_eq!(guard.pin_count(), 1);
    }
    assert_eq!(pool.pin_count(first_frame).expect("pin count"), 0);
}

#[test]
fn new_page_allocates_resident_frame() {
    let mut pool = pool(2);
    let (page_id, frame_id) = {
        let (page_id, guard) = pool
            .new_page(valid_contract(PageId::new(12), Lsn::new(12)))
            .expect("new page");
        (page_id, guard.frame_id())
    };

    assert_eq!(page_id, PageId::new(12));
    assert_eq!(pool.resident_frame_id(PageId::new(12)), Some(frame_id));
    assert_eq!(pool.pin_count(frame_id).expect("pin count"), 0);
}

#[test]
fn full_pool_evicts_clean_unpinned_page_through_clock() {
    let mut pool = pool(1);
    {
        let _ = pool
            .new_page(valid_contract(PageId::new(20), Lsn::new(20)))
            .expect("first page");
    }
    let first_frame = pool
        .resident_frame_id(PageId::new(20))
        .expect("first resident");

    {
        let (_page_id, guard) = pool
            .new_page(valid_contract(PageId::new(21), Lsn::new(21)))
            .expect("second page evicts first");
        assert_eq!(guard.frame_id(), first_frame);
    }

    assert!(!pool.contains_resident_page(PageId::new(20)));
    assert_eq!(pool.resident_frame_id(PageId::new(21)), Some(first_frame));
    assert_eq!(
        pool.frame_page_id(first_frame).expect("frame page"),
        Some(PageId::new(21))
    );
}

#[test]
fn pinned_exhaustion_returns_explicit_storage_error() {
    let mut pool = pool(1);
    let frame_id = {
        let (_page_id, guard) = pool
            .new_page(valid_contract(PageId::new(30), Lsn::new(30)))
            .expect("pinned first");
        guard.frame_id()
    };
    pool.frames[frame_id.zero_based_index()]
        .pin()
        .expect("pin resident frame");

    let error = pool
        .new_page(valid_contract(PageId::new(31), Lsn::new(31)))
        .expect_err("no victim while first page is pinned");

    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(error.message().contains("every resident frame is pinned"));
}

#[test]
fn dirty_exhaustion_returns_explicit_storage_error_and_tracker_is_synced() {
    let mut pool = pool(1);
    {
        let (_page_id, mut guard) = pool
            .new_page(valid_contract(PageId::new(40), Lsn::new(40)))
            .expect("new page");
        guard.mark_dirty(Lsn::new(40)).expect("dirty mark");
    }

    assert!(pool.is_dirty(PageId::new(40)).expect("dirty tracked"));
    let error = pool
        .new_page(valid_contract(PageId::new(41), Lsn::new(41)))
        .expect_err("dirty page cannot be evicted");

    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(error.message().contains("no clean unpinned resident frame"));
    assert!(pool.contains_resident_page(PageId::new(40)));
}

#[test]
fn frame_table_updates_on_eviction() {
    let mut pool = pool(2);
    {
        let _ = pool
            .new_page(valid_contract(PageId::new(50), Lsn::new(50)))
            .expect("first page");
    }
    {
        let _ = pool
            .new_page(valid_contract(PageId::new(51), Lsn::new(51)))
            .expect("second page");
    }
    let evicted_frame = pool
        .resident_frame_id(PageId::new(50))
        .expect("first resident");

    {
        let _ = pool
            .new_page(valid_contract(PageId::new(52), Lsn::new(52)))
            .expect("third page evicts first by Clock order");
    }

    assert!(!pool.contains_resident_page(PageId::new(50)));
    assert_eq!(pool.resident_frame_id(PageId::new(52)), Some(evicted_frame));
    assert_eq!(pool.resident_page_count(), 2);
}
