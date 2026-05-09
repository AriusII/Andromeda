#![forbid(unsafe_code)]

use andromeda_buffer_pool::{
    BufferPool, BufferPoolConfig, BufferPoolManager, TestWalDurabilityObserver,
};
use andromeda_error::{AndromedaErrorKind, AndromedaResult};
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

fn pool(frame_count: usize) -> BufferPool<InMemoryPageStore> {
    BufferPool::new(
        BufferPoolConfig::new(frame_count, PageSize::KiB16).expect("valid buffer pool config"),
        InMemoryPageStore::new(PageSize::KiB16),
    )
    .expect("valid buffer pool")
}

fn store_with_pages(pages: &[(PageId, Lsn)]) -> InMemoryPageStore {
    let mut store = InMemoryPageStore::new(PageSize::KiB16);
    for (page_id, page_lsn) in pages {
        store
            .allocate_page(valid_contract(*page_id, *page_lsn), *page_lsn)
            .expect("test page allocation is valid");
    }
    store
}

#[derive(Debug)]
struct RecordingPageStore {
    inner: InMemoryPageStore,
    writes: Vec<(PageId, Lsn)>,
}

impl RecordingPageStore {
    fn new() -> Self {
        Self {
            inner: InMemoryPageStore::new(PageSize::KiB16),
            writes: Vec::new(),
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
        let page_id = image
            .page_id()
            .expect("validated page image exposes page id");
        self.writes.push((page_id, durable_lsn));
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
fn pinned_frame_is_not_reused_until_unpinned() {
    let first_page_id = PageId::new(10);
    let second_page_id = PageId::new(11);
    let store = store_with_pages(&[
        (first_page_id, Lsn::new(100)),
        (second_page_id, Lsn::new(101)),
    ]);
    let mut pool = BufferPool::new(
        BufferPoolConfig::new(1, PageSize::KiB16).expect("valid single-frame pool"),
        store,
    )
    .expect("valid buffer pool");

    let first_frame =
        BufferPoolManager::pin_page(&mut pool, first_page_id).expect("first page pins");
    assert_eq!(
        pool.pin_count(first_frame).expect("pin count is visible"),
        1
    );
    assert!(pool.contains_resident_page(first_page_id));

    let error = BufferPoolManager::pin_page(&mut pool, second_page_id)
        .expect_err("pinned resident frame cannot be evicted for another page");
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(pool.contains_resident_page(first_page_id));
    assert!(!pool.contains_resident_page(second_page_id));

    BufferPoolManager::unpin_page(&mut pool, first_frame, None).expect("release first page pin");
    let second_frame =
        BufferPoolManager::pin_page(&mut pool, second_page_id).expect("clean frame can be reused");

    assert_eq!(
        pool.frame_page_id(second_frame)
            .expect("resident frame exposes page id"),
        Some(second_page_id)
    );
    assert!(!pool.contains_resident_page(first_page_id));
    assert!(pool.contains_resident_page(second_page_id));
}

#[test]
fn dirty_tracking_preserves_first_and_latest_lsn_for_wal_fence() {
    let mut pool = pool(2);
    let page_id = PageId::new(20);
    let first_dirty_lsn = Lsn::new(100);
    let latest_dirty_lsn = Lsn::new(150);

    {
        let (_page_id, mut guard) = pool
            .new_page(valid_contract(page_id, first_dirty_lsn))
            .expect("new page is resident");
        guard
            .mark_dirty(first_dirty_lsn)
            .expect("first dirty mark is valid");
        guard
            .mark_dirty(latest_dirty_lsn)
            .expect("later dirty mark advances WAL fence");
    }

    assert_eq!(
        pool.dirty_tracker()
            .first_dirty_lsn(page_id)
            .expect("dirty tracker query succeeds"),
        Some(first_dirty_lsn)
    );
    assert_eq!(
        pool.dirty_tracker()
            .last_dirty_lsn(page_id)
            .expect("dirty tracker query succeeds"),
        Some(latest_dirty_lsn)
    );

    let observer = TestWalDurabilityObserver::with_durable_lsn(first_dirty_lsn.get());
    let report = pool
        .flush_all_dirty_with_report(&observer)
        .expect("flush report succeeds");

    assert_eq!(report.flushed, 0);
    assert!(report.errors.is_empty());
    assert_eq!(report.blocked_by_wal_durability.len(), 1);
    assert_eq!(
        report.blocked_by_wal_durability[0].first_dirty_lsn,
        first_dirty_lsn
    );
    assert_eq!(
        report.blocked_by_wal_durability[0].last_dirty_lsn,
        latest_dirty_lsn
    );
    assert!(pool.is_dirty(page_id).expect("blocked page stays dirty"));
}

#[test]
fn checkpoint_flush_candidates_use_oldest_dirty_lsn_then_page_id_order() {
    let mut pool = BufferPool::new(
        BufferPoolConfig::new(3, PageSize::KiB16).expect("valid buffer pool config"),
        RecordingPageStore::new(),
    )
    .expect("valid buffer pool");
    let pages = [
        (PageId::new(30), Lsn::new(300)),
        (PageId::new(10), Lsn::new(100)),
        (PageId::new(20), Lsn::new(200)),
    ];

    for (page_id, page_lsn) in pages {
        let (_page_id, mut guard) = pool
            .new_page(valid_contract(page_id, page_lsn))
            .expect("new page is resident");
        guard.mark_dirty(page_lsn).expect("dirty mark is valid");
    }

    let candidates = pool.dirty_tracker().ordered_flush_candidates();
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.page_id())
            .collect::<Vec<_>>(),
        vec![PageId::new(10), PageId::new(20), PageId::new(30)]
    );

    let observer = TestWalDurabilityObserver::with_durable_lsn(300);
    let report = pool
        .flush_all_dirty_with_report(&observer)
        .expect("checkpoint-style flush report succeeds");

    assert_eq!(report.flushed, 3);
    assert!(report.blocked_by_wal_durability.is_empty());
    assert!(report.errors.is_empty());

    let store = pool.into_page_store();
    assert_eq!(
        store
            .writes
            .iter()
            .map(|(page_id, _durable_lsn)| *page_id)
            .collect::<Vec<_>>(),
        vec![PageId::new(10), PageId::new(20), PageId::new(30)]
    );
    assert_eq!(
        store
            .writes
            .iter()
            .map(|(_page_id, durable_lsn)| *durable_lsn)
            .collect::<Vec<_>>(),
        vec![Lsn::new(300), Lsn::new(300), Lsn::new(300)]
    );
}

#[test]
fn legacy_flush_without_wal_observer_does_not_clear_dirty_state() {
    let mut pool = pool(2);
    let page_id = PageId::new(40);
    let page_lsn = Lsn::new(100);

    {
        let (_page_id, mut guard) = pool
            .new_page(valid_contract(page_id, page_lsn))
            .expect("new page is resident");
        guard.mark_dirty(page_lsn).expect("dirty mark is valid");
    }

    let error = BufferPoolManager::flush_all_dirty(&mut pool)
        .expect_err("dirty flush requires WAL durability observer");

    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(pool.is_dirty(page_id).expect("page remains dirty"));
}
