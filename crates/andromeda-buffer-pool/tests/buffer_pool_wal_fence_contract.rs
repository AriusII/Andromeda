#![forbid(unsafe_code)]

use andromeda_buffer_pool::{
    BufferPool, BufferPoolConfig, BufferPoolManager, FlushError, FlushStorageOperation,
    TestWalDurabilityObserver,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_storage_page::{
    AllocationId, InMemoryPageStore, Lsn, ObjectId, PageFlags, PageHeader, PageId, PageImage,
    PageLayoutContract, PageSize, PageStore, PageTrailer, PageType,
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

#[derive(Debug)]
struct RecordingPageStore {
    inner: InMemoryPageStore,
    fail_write_count: usize,
    last_write_durable_lsn: Option<Lsn>,
    write_count: usize,
}

impl RecordingPageStore {
    fn new(fail_write_count: usize) -> Self {
        Self {
            inner: InMemoryPageStore::new(PageSize::KiB16),
            fail_write_count,
            last_write_durable_lsn: None,
            write_count: 0,
        }
    }
}

impl PageStore for RecordingPageStore {
    fn page_size(&self) -> PageSize {
        self.inner.page_size()
    }

    fn read_page(&self, page_id: PageId) -> AndromedaResult<Option<PageImage>> {
        self.inner.read_page(page_id)
    }

    fn write_page(&mut self, image: PageImage, durable_lsn: Lsn) -> AndromedaResult<()> {
        self.last_write_durable_lsn = Some(durable_lsn);
        self.write_count += 1;
        if self.fail_write_count > 0 {
            self.fail_write_count -= 1;
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "injected disk page store write failure",
            ));
        }
        self.inner.write_page(image, durable_lsn)
    }

    fn allocate_page(
        &mut self,
        layout_contract: PageLayoutContract,
        durable_lsn: Lsn,
    ) -> AndromedaResult<PageImage> {
        self.inner.allocate_page(layout_contract, durable_lsn)
    }
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
fn dirty_page_flush_passes_observed_durable_lsn_to_page_store() {
    let mut pool = BufferPool::new(
        BufferPoolConfig::new(2, PageSize::KiB16).expect("valid buffer pool config"),
        RecordingPageStore::new(0),
    )
    .expect("valid buffer pool");
    let page_id = PageId::new(12);
    let page_lsn = Lsn::new(100);

    {
        let (_page_id, mut guard) = pool
            .new_page(valid_contract(page_id, page_lsn))
            .expect("new page is resident");
        guard.mark_dirty(page_lsn).expect("page dirty at LSN 100");
    }

    let observer = TestWalDurabilityObserver::with_durable_lsn(150);
    let flushed = pool
        .flush_all_dirty_with_report(&observer)
        .expect("flush report succeeds");

    assert_eq!(flushed.flushed, 1);
    assert!(flushed.blocked_by_wal_durability.is_empty());
    assert!(flushed.errors.is_empty());

    let store = pool.into_page_store();
    assert_eq!(store.last_write_durable_lsn, Some(Lsn::new(150)));
    assert_eq!(store.write_count, 1);
}

#[test]
fn dirty_page_flush_waits_for_latest_dirty_lsn_not_first_dirty_lsn() {
    let mut pool = BufferPool::new(
        BufferPoolConfig::new(2, PageSize::KiB16).expect("valid buffer pool config"),
        RecordingPageStore::new(0),
    )
    .expect("valid buffer pool");
    let page_id = PageId::new(14);
    let page_lsn = Lsn::new(100);
    let later_dirty_lsn = Lsn::new(150);

    {
        let (_page_id, mut guard) = pool
            .new_page(valid_contract(page_id, page_lsn))
            .expect("new page is resident");
        guard.mark_dirty(page_lsn).expect("page dirty at LSN 100");
        guard
            .mark_dirty(later_dirty_lsn)
            .expect("same page dirtied again at LSN 150");
    }

    let observer = TestWalDurabilityObserver::with_durable_lsn(100);
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
        blocked.blocked_by_wal_durability[0].last_dirty_lsn,
        later_dirty_lsn
    );
    assert_eq!(blocked.blocked_by_wal_durability[0].lsn_gap(), 50);
    assert!(pool.is_dirty(page_id).expect("page remains dirty"));

    observer.set_durable_lsn(150);
    let flushed = pool
        .flush_all_dirty_with_report(&observer)
        .expect("flush report succeeds after latest dirty LSN is durable");

    assert_eq!(flushed.flushed, 1);
    assert!(flushed.blocked_by_wal_durability.is_empty());
    assert!(flushed.errors.is_empty());

    let store = pool.into_page_store();
    assert_eq!(store.write_count, 1);
    assert_eq!(store.last_write_durable_lsn, Some(Lsn::new(150)));
}

#[test]
fn failed_page_store_flush_keeps_dirty_frame_retryable() {
    let mut pool = BufferPool::new(
        BufferPoolConfig::new(2, PageSize::KiB16).expect("valid buffer pool config"),
        RecordingPageStore::new(1),
    )
    .expect("valid buffer pool");
    let page_id = PageId::new(13);
    let page_lsn = Lsn::new(100);

    {
        let (_page_id, mut guard) = pool
            .new_page(valid_contract(page_id, page_lsn))
            .expect("new page is resident");
        guard.mark_dirty(page_lsn).expect("page dirty at LSN 100");
    }

    let observer = TestWalDurabilityObserver::with_durable_lsn(100);
    let failed = pool
        .flush_all_dirty_with_report(&observer)
        .expect("flush report records storage error");

    assert_eq!(failed.flushed, 0);
    assert!(failed.blocked_by_wal_durability.is_empty());
    assert_eq!(failed.errors.len(), 1);
    assert!(matches!(
        &failed.errors[0],
        FlushError::StorageError {
            page_id: failed_page_id,
            operation: FlushStorageOperation::PageStoreWrite,
            message,
        } if *failed_page_id == page_id
            && message.contains("injected disk page store write failure")
    ));
    assert!(pool.is_dirty(page_id).expect("page remains dirty"));

    let retried = pool
        .flush_all_dirty_with_report(&observer)
        .expect("dirty page remains retryable after storage error");

    assert_eq!(retried.flushed, 1);
    assert!(retried.blocked_by_wal_durability.is_empty());
    assert!(retried.errors.is_empty());
    assert!(!pool.is_dirty(page_id).expect("page is clean after retry"));
}

#[test]
fn page_store_error_does_not_stop_other_eligible_dirty_flushes() {
    let mut pool = BufferPool::new(
        BufferPoolConfig::new(2, PageSize::KiB16).expect("valid buffer pool config"),
        RecordingPageStore::new(1),
    )
    .expect("valid buffer pool");
    let first_page_id = PageId::new(20);
    let second_page_id = PageId::new(21);

    {
        let (_page_id, mut guard) = pool
            .new_page(valid_contract(first_page_id, Lsn::new(100)))
            .expect("first page is resident");
        guard.mark_dirty(Lsn::new(100)).expect("first page dirty");
    }
    {
        let (_page_id, mut guard) = pool
            .new_page(valid_contract(second_page_id, Lsn::new(101)))
            .expect("second page is resident");
        guard.mark_dirty(Lsn::new(101)).expect("second page dirty");
    }

    let observer = TestWalDurabilityObserver::with_durable_lsn(101);
    let report = pool
        .flush_all_dirty_with_report(&observer)
        .expect("flush report records storage error and keeps scanning");

    assert_eq!(report.flushed, 1);
    assert!(report.blocked_by_wal_durability.is_empty());
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].page_id(), Some(first_page_id));
    assert!(
        pool.is_dirty(first_page_id)
            .expect("first page remains dirty")
    );
    assert!(!pool.is_dirty(second_page_id).expect("second page is clean"));
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
