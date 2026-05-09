#![forbid(unsafe_code)]

use andromeda_buffer_pool::{BufferPool, BufferPoolConfig, TestWalDurabilityObserver};
use andromeda_error::AndromedaErrorKind;
use andromeda_storage_page::{
    AllocationId, InMemoryPageStore, Lsn, ObjectId, PageFlags, PageHeader, PageId, PageImage,
    PageLayoutContract, PageSize, PageStore, PageTrailer, PageType,
};

const PAGE_LIFECYCLE_SPEC: &str = include_str!("../../../documentations/specs/PageLifecycle_v0.md");
const BUFFER_POOL_POLICY_SPEC: &str =
    include_str!("../../../documentations/specs/BufferPoolPolicy_v0.md");

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

#[test]
fn page_layout_rejects_zero_lsn_before_residency() {
    let invalid = valid_contract(PageId::new(1), Lsn::ZERO);

    let layout_error = invalid
        .validate()
        .expect_err("page layout with zero LSN must fail");
    assert_eq!(layout_error.kind(), AndromedaErrorKind::Storage);

    let image_error = PageImage::zeroed_with_layout(invalid)
        .expect_err("page image admission also rejects zero page LSN");
    assert_eq!(image_error.kind(), AndromedaErrorKind::Storage);
}

#[test]
fn page_store_rejects_flush_when_durable_wal_lags_page_lsn() {
    let mut store = InMemoryPageStore::new(PageSize::KiB16);
    let page_id = PageId::new(2);
    let page_lsn = Lsn::new(100);
    let image = store
        .allocate_page(valid_contract(page_id, page_lsn), page_lsn)
        .expect("allocation with durable page LSN succeeds");

    let error = store
        .write_page(image, Lsn::new(99))
        .expect_err("page-store flush must fail while durable WAL lags page LSN");

    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(
        error
            .message()
            .contains("WAL-before-page-flush precondition failed")
    );
}

#[test]
fn demand_read_of_missing_page_has_no_allocation_or_dirty_side_effect() {
    let store = InMemoryPageStore::new(PageSize::KiB16);
    let missing_page = PageId::new(3);

    assert_eq!(
        store
            .read_page(missing_page)
            .expect("missing demand read succeeds"),
        None
    );
    assert_eq!(store.page_count(), 0);
    assert!(!store.contains_page(missing_page));
}

#[test]
fn dirty_page_is_not_flushed_or_cleaned_before_wal_durable() {
    let mut pool = BufferPool::new(
        BufferPoolConfig::new(2, PageSize::KiB16).expect("valid buffer pool config"),
        InMemoryPageStore::new(PageSize::KiB16),
    )
    .expect("valid buffer pool");
    let page_id = PageId::new(4);
    let page_lsn = Lsn::new(100);

    {
        let (_page_id, mut guard) = pool
            .new_page(valid_contract(page_id, page_lsn))
            .expect("new page is resident");
        guard.mark_dirty(page_lsn).expect("dirty mark is valid");
    }

    let observer = TestWalDurabilityObserver::with_durable_lsn(99);
    let blocked = pool
        .flush_all_dirty_with_report(&observer)
        .expect("flush report succeeds even when blocked");

    assert_eq!(blocked.flushed, 0);
    assert_eq!(blocked.blocked_by_wal_durability.len(), 1);
    assert!(blocked.errors.is_empty());
    assert!(pool.is_dirty(page_id).expect("WAL-lagged page stays dirty"));

    observer.set_durable_lsn(100);
    let flushed = pool
        .flush_all_dirty_with_report(&observer)
        .expect("flush succeeds after WAL is durable");

    assert_eq!(flushed.flushed, 1);
    assert!(flushed.blocked_by_wal_durability.is_empty());
    assert!(flushed.errors.is_empty());
    assert!(!pool.is_dirty(page_id).expect("flushed page becomes clean"));
}

#[test]
fn lifecycle_specs_mark_missing_runtime_owners_honestly() {
    assert!(PAGE_LIFECYCLE_SPEC.contains("Missing implementation owner: read-ahead scheduler"));
    assert!(PAGE_LIFECYCLE_SPEC.contains("Missing implementation owner: checkpoint scheduler"));
    assert!(
        PAGE_LIFECYCLE_SPEC.contains("Missing implementation owner: commit visibility coordinator")
    );
    assert!(PAGE_LIFECYCLE_SPEC.contains("No visible commit before durable WAL"));
    assert!(BUFFER_POOL_POLICY_SPEC.contains("Read-ahead is advisory only"));
    assert!(
        BUFFER_POOL_POLICY_SPEC
            .contains("The buffer pool is not the commit visibility coordinator")
    );
}
