use andromeda_error::AndromedaErrorKind;

use crate::Lsn;

use super::*;

fn valid_header() -> PageHeader {
    PageHeader {
        magic: PageHeader::MAGIC,
        format_version: PageHeader::FORMAT_VERSION_V0,
        page_size: PageSize::KiB16,
        page_type: PageType::FixedRow,
        page_id: PageId::new(101),
        object_id: ObjectId::new(7),
        allocation_id: AllocationId::new(9),
        page_lsn: Lsn::new(10),
        page_epoch: 1,
        previous_page_id: Some(PageId::new(100)),
        next_page_id: Some(PageId::new(102)),
        header_len: PageHeader::MIN_HEADER_LEN_V0,
        payload_offset: 128,
        payload_len: 1024,
        free_start: 512,
        free_end: 768,
        free_bytes: 256,
        slot_count: 2,
        row_count: 2,
        flags: PageFlags::HAS_PREVIOUS.with(PageFlags::HAS_NEXT),
        header_crc: 7,
    }
}

fn valid_trailer() -> PageTrailer {
    PageTrailer {
        payload_crc64: 11,
        page_hash: [3; 32],
        torn_write_guard: 13,
    }
}

fn valid_contract(page_id: PageId, page_size: PageSize, page_lsn: Lsn) -> PageLayoutContract {
    PageLayoutContract {
        header: PageHeader {
            page_id,
            page_size,
            page_lsn,
            ..valid_header()
        },
        trailer: valid_trailer(),
    }
}

#[test]
fn page_header_and_trailer_have_test_vectors() {
    assert!(valid_header().validate().is_ok());
    assert!(valid_trailer().validate().is_ok());
}

#[test]
fn page_layout_contract_rejects_invalid_offsets_and_rows() {
    let mut header = valid_header();
    header.free_end = header.payload_offset + header.payload_len + 1;
    assert_eq!(
        header.validate().unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );

    let mut header = valid_header();
    header.row_count = u32::from(header.slot_count) + 1;
    assert_eq!(
        header.validate().unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn page_layout_contract_rejects_identity_and_guard_failures() {
    let mut header = valid_header();
    header.page_id = PageId::new(0);
    assert_eq!(
        header.validate().unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );

    let contract = PageLayoutContract {
        header: valid_header(),
        trailer: PageTrailer {
            torn_write_guard: valid_header().page_id.get(),
            ..valid_trailer()
        },
    };
    assert_eq!(
        contract.validate().unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn page_store_page_image_accepts_exact_16k_and_32k_lengths() {
    let image_16k =
        PageImage::new(PageSize::KiB16, vec![1; 16 * 1024]).expect("16KiB image is canonical");
    assert_eq!(image_16k.page_size(), PageSize::KiB16);
    assert_eq!(image_16k.len(), 16 * 1024);
    assert_eq!(image_16k.page_id(), None);
    assert_eq!(image_16k.page_lsn(), None);

    let contract = valid_contract(PageId::new(202), PageSize::KiB32, Lsn::new(22));
    let image_32k = PageImage::with_layout(contract, vec![2; 32 * 1024])
        .expect("32KiB image with valid layout is canonical");
    assert_eq!(image_32k.page_size(), PageSize::KiB32);
    assert_eq!(image_32k.len(), 32 * 1024);
    assert_eq!(image_32k.page_id(), Some(PageId::new(202)));
    assert_eq!(image_32k.page_lsn(), Some(Lsn::new(22)));
    assert_eq!(image_32k.layout_contract(), Some(contract));
}

#[test]
fn page_store_page_image_rejects_wrong_lengths_and_invalid_layout() {
    assert_eq!(
        PageImage::new(PageSize::KiB16, vec![0; (16 * 1024) - 1])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );
    assert_eq!(
        PageImage::new(PageSize::KiB32, vec![0; (32 * 1024) + 1])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );

    let invalid_contract = valid_contract(PageId::new(0), PageSize::KiB16, Lsn::new(12));
    assert_eq!(
        PageImage::with_layout(invalid_contract, vec![0; 16 * 1024])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );

    let mismatched_len_contract = valid_contract(PageId::new(203), PageSize::KiB32, Lsn::new(23));
    assert_eq!(
        PageImage::with_layout(mismatched_len_contract, vec![0; 16 * 1024])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn page_store_in_memory_allocate_read_write_is_deterministic() {
    let mut store = InMemoryPageStore::new(PageSize::KiB16);
    let contract = valid_contract(PageId::new(301), PageSize::KiB16, Lsn::new(30));

    assert_eq!(store.page_size(), PageSize::KiB16);
    assert_eq!(store.page_count(), 0);
    assert_eq!(
        store.read_page(PageId::new(301)).expect("read succeeds"),
        None
    );

    let allocated = store
        .allocate_page(contract, Lsn::new(30))
        .expect("allocation respects WAL precondition");
    assert_eq!(allocated.page_id(), Some(PageId::new(301)));
    assert_eq!(allocated.page_lsn(), Some(Lsn::new(30)));
    assert_eq!(store.page_count(), 1);
    assert!(store.contains_page(PageId::new(301)));
    assert_eq!(
        store.read_page(PageId::new(301)).expect("read allocated"),
        Some(allocated.clone())
    );

    let updated_contract = valid_contract(PageId::new(301), PageSize::KiB16, Lsn::new(31));
    let updated = PageImage::with_layout(updated_contract, vec![9; 16 * 1024])
        .expect("updated full-page image");
    store
        .write_page(updated.clone(), Lsn::new(31))
        .expect("write respects WAL precondition");
    assert_eq!(
        store.read_page(PageId::new(301)).expect("read updated"),
        Some(updated)
    );
}

#[test]
fn page_store_rejects_unallocated_writes_duplicate_allocations_and_size_mismatch() {
    let mut store = InMemoryPageStore::new(PageSize::KiB16);
    let first_contract = valid_contract(PageId::new(401), PageSize::KiB16, Lsn::new(40));
    store
        .allocate_page(first_contract, Lsn::new(40))
        .expect("initial allocation");

    assert_eq!(
        store
            .allocate_page(first_contract, Lsn::new(40))
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );

    let unallocated = PageImage::with_layout(
        valid_contract(PageId::new(402), PageSize::KiB16, Lsn::new(41)),
        vec![4; 16 * 1024],
    )
    .expect("unallocated page image");
    assert_eq!(
        store
            .write_page(unallocated, Lsn::new(41))
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );

    let wrong_size_contract = valid_contract(PageId::new(403), PageSize::KiB32, Lsn::new(42));
    assert_eq!(
        store
            .allocate_page(wrong_size_contract, Lsn::new(42))
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );

    assert_eq!(
        store.read_page(PageId::new(0)).unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn page_store_enforces_wal_before_page_flush_precondition() {
    let mut store = InMemoryPageStore::new(PageSize::KiB16);
    let contract = valid_contract(PageId::new(501), PageSize::KiB16, Lsn::new(50));

    assert_eq!(
        store
            .allocate_page(contract, Lsn::new(49))
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );
    assert_eq!(store.page_count(), 0);

    let allocated = store
        .allocate_page(contract, Lsn::new(50))
        .expect("allocation succeeds once WAL durable LSN reaches page LSN");
    assert_eq!(allocated.page_lsn(), Some(Lsn::new(50)));

    let updated = PageImage::with_layout(
        valid_contract(PageId::new(501), PageSize::KiB16, Lsn::new(51)),
        vec![5; 16 * 1024],
    )
    .expect("updated image");
    assert_eq!(
        store
            .write_page(updated.clone(), Lsn::new(50))
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );

    store
        .write_page(updated, Lsn::new(51))
        .expect("write succeeds once WAL is durable through page LSN");
}
