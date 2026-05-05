#![forbid(unsafe_code)]

use andromeda_core::AndromedaErrorKind;
use andromeda_storage::{
    AllocationId, BufferPool, BufferPoolConfig, BufferPoolManager, InMemoryPageStore, Lsn,
    ObjectId, PageFlags, PageHeader, PageId, PageLayoutContract, PageSize, PageTrailer, PageType,
    TestWalDurabilityObserver,
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

fn pool() -> BufferPool<InMemoryPageStore> {
    BufferPool::new(
        BufferPoolConfig::new(2, PageSize::KiB16).expect("valid buffer pool config"),
        InMemoryPageStore::new(PageSize::KiB16),
    )
    .expect("valid buffer pool")
}

#[test]
fn dirty_page_flush_is_blocked_until_page_lsn_is_wal_durable() {
    let mut pool = pool();
    let page_id = PageId::new(10);
    let page_lsn = Lsn::new(100);

    {
        let (_page_id, mut guard) = pool
            .new_page(valid_contract(page_id, page_lsn))
            .expect("new page is resident");
        guard.mark_dirty(page_lsn).expect("page dirty at LSN 100");
    }

    let observer = TestWalDurabilityObserver::with_durable_lsn(99);
    let blocked = pool
        .flush_all_dirty_with_report(&observer)
        .expect("flush report succeeds");

    assert_eq!(blocked.flushed, 0);
    assert!(blocked.errors.is_empty());
    assert_eq!(blocked.blocked_by_wal_durability.len(), 1);
    assert_eq!(blocked.blocked_by_wal_durability[0].page_id, page_id);
    assert_eq!(
        blocked.blocked_by_wal_durability[0].first_dirty_lsn,
        page_lsn
    );
    assert_eq!(
        blocked.blocked_by_wal_durability[0].max_durable_lsn,
        Lsn::new(99)
    );
    assert!(pool.is_dirty(page_id).expect("page remains dirty"));

    observer.set_durable_lsn(100);
    let flushed = pool
        .flush_all_dirty_with_report(&observer)
        .expect("flush report succeeds once WAL is durable through page LSN");

    assert_eq!(flushed.flushed, 1);
    assert!(flushed.blocked_by_wal_durability.is_empty());
    assert!(flushed.errors.is_empty());
    assert!(!pool.is_dirty(page_id).expect("page is clean after flush"));
}

#[test]
fn legacy_flush_all_dirty_rejects_dirty_pages_without_wal_observer() {
    let mut pool = pool();
    let page_id = PageId::new(11);
    let page_lsn = Lsn::new(100);

    {
        let (_page_id, mut guard) = pool
            .new_page(valid_contract(page_id, page_lsn))
            .expect("new page is resident");
        guard.mark_dirty(page_lsn).expect("page dirty at LSN 100");
    }

    let error = BufferPoolManager::flush_all_dirty(&mut pool)
        .expect_err("dirty flush requires WAL durability observer");

    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(
        error
            .message()
            .contains("WAL durability observer is required")
    );
    assert!(pool.is_dirty(page_id).expect("page remains dirty"));
}
