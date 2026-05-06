#![forbid(unsafe_code)]

use andromeda_storage::{
    AllocationId, BufferPool, BufferPoolConfig, DiskPageStore, ExtentDescriptor, ExtentId,
    ExtentState, Lsn, ObjectId, PageFlags, PageHeader, PageId, PageLayoutContract, PageSize,
    PageType, TestWalDurabilityObserver, integrity_trailer_for_payload,
};

fn test_extent() -> ExtentDescriptor {
    ExtentDescriptor {
        extent_id: ExtentId::new(1),
        object_id: ObjectId::new(2),
        allocation_id: AllocationId::new(3),
        first_page_id: PageId::new(1),
        page_count: 4,
        page_size: PageSize::KiB16,
        state: ExtentState::AllocatingHot,
        segment_id: None,
        file_offset: 0,
        allocated_on_disk: false,
    }
}

fn page_contract(page_id: PageId, page_lsn: Lsn) -> PageLayoutContract {
    let header = PageHeader {
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
    };
    let payload = vec![0; header.payload_len as usize];
    PageLayoutContract {
        header,
        trailer: integrity_trailer_for_payload(&header, &payload),
    }
}

#[test]
fn buffer_pool_flushes_disk_page_store_and_reopens_page_layout() {
    let temp_dir = tempfile::TempDir::new().expect("temp dir");
    let data_file = temp_dir.path().join("pages.bin");
    let temp_io_dir = temp_dir.path().join("io");
    let extent = test_extent();
    let page_id = PageId::new(1);
    let page_lsn = Lsn::new(100);
    let contract = page_contract(page_id, page_lsn);

    {
        let mut store = DiskPageStore::new(&data_file, &temp_io_dir).expect("disk page store");
        store.allocate_extent(extent).expect("extent allocated");
        assert!(
            store
                .read_page(page_id)
                .expect("allocated extent read")
                .is_none(),
            "extent allocation alone must not expose a page image"
        );

        let mut pool = BufferPool::new(
            BufferPoolConfig::new(2, PageSize::KiB16).expect("buffer pool config"),
            store,
        )
        .expect("buffer pool over disk page store");

        {
            let (created_page_id, mut guard) = pool.new_page(contract).expect("page created");
            assert_eq!(created_page_id, page_id);
            assert_eq!(guard.layout_contract(), Some(contract));
            guard.mark_dirty(page_lsn).expect("dirty at LSN 100");
        }

        let observer = TestWalDurabilityObserver::with_durable_lsn(99);
        let blocked = pool
            .flush_all_dirty_with_report(&observer)
            .expect("blocked flush report");
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
            .expect("durable flush report");
        assert_eq!(flushed.flushed, 1);
        assert!(flushed.blocked_by_wal_durability.is_empty());
        assert!(flushed.errors.is_empty());
        assert!(!pool.is_dirty(page_id).expect("page is clean"));

        drop(pool.into_page_store());
    }

    let mut reopened_store = DiskPageStore::new(&data_file, &temp_io_dir).expect("reopen store");
    reopened_store
        .register_extent(extent)
        .expect("re-register extent metadata");

    let direct = reopened_store
        .read_page(page_id)
        .expect("read persisted page")
        .expect("page exists after flush");
    assert_eq!(direct.layout_contract(), Some(contract));
    assert_eq!(direct.page_id(), Some(page_id));
    assert_eq!(direct.page_lsn(), Some(page_lsn));
    assert_eq!(direct.as_bytes()[0..4], PageHeader::MAGIC.to_le_bytes());
    assert_eq!(direct.as_bytes()[6], 1);
    assert_eq!(direct.as_bytes()[7], 1);
    assert_eq!(direct.as_bytes()[8..16], page_id.get().to_le_bytes());
    assert_eq!(direct.len(), PageSize::KiB16.bytes_usize());

    let mut reopened_pool = BufferPool::new(
        BufferPoolConfig::new(1, PageSize::KiB16).expect("buffer pool config"),
        reopened_store,
    )
    .expect("buffer pool after reopen");
    let guard = reopened_pool
        .fetch_page(page_id)
        .expect("fetch reopened page");
    assert_eq!(guard.layout_contract(), Some(contract));
    assert_eq!(
        guard.image().expect("resident image").as_bytes(),
        direct.as_bytes()
    );
}
