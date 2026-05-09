use andromeda_disk_page_store::DiskPageStore;
use andromeda_segment::{ExtentDescriptor, ExtentId, ExtentState};
use andromeda_storage_heap::{HeapPage, HeapPageInsert};
use andromeda_storage_page::{
    AllocationId, Lsn, ObjectId, PAGE_CODEC_V1_HEADER_LEN, PageCodecV1, PageFlags, PageHeader,
    PageId, PageImage, PageLayoutContract, PageSize, PageStore, PageType,
    integrity_trailer_for_payload,
};

use crate::support::{
    HEADER_SIZE, SLOT_ENTRY_SIZE, SLOT_METADATA_SIZE, TRAILER_SIZE, heap_v1_image_with_slot0_tuple,
    metadata_offset, slot_base, slot_offset,
};

#[test]
fn test_heap_page_v1_candidate_layout_constants() {
    let page_size = PageSize::KiB16;
    let page_bytes = page_size.bytes_usize();

    assert_eq!(PageHeader::MIN_HEADER_LEN_V0, 96);
    assert_eq!(HEADER_SIZE, PAGE_CODEC_V1_HEADER_LEN);
    assert_eq!(
        HEADER_SIZE, 112,
        "HeapPageV1 tuple payload reserves PageCodecV1/DiskPageStore header overlays"
    );
    assert_eq!(TRAILER_SIZE, 48);
    assert_eq!(
        metadata_offset(page_size),
        page_bytes - 52,
        "HeapPageV1 metadata starts at page_size - 52"
    );
    assert_eq!(
        slot_offset(page_size, 0),
        page_bytes - 57,
        "HeapPageV1 slot 0 starts at page_size - 57"
    );
}

#[test]
fn test_heap_page_v1_tuple_payload_starts_after_durable_header_and_grows_upward() {
    let mut page = HeapPageInsert::new(PageId::new(7), PageSize::KiB16)
        .expect("create heap page insert context");

    let slot0 = page.insert_raw_tuple(b"abc").expect("insert slot 0");
    let slot1 = page.insert_raw_tuple(b"defgh").expect("insert slot 1");
    assert_eq!((slot0, slot1), (0, 1));

    let image = page.serialize().expect("serialize heap page");
    assert_eq!(&image[HEADER_SIZE..HEADER_SIZE + 3], b"abc");
    assert_eq!(&image[HEADER_SIZE + 3..HEADER_SIZE + 8], b"defgh");

    let slot0_entry = &image[slot_offset(PageSize::KiB16, 0)..slot_offset(PageSize::KiB16, 0) + 5];
    let slot1_entry = &image[slot_offset(PageSize::KiB16, 1)..slot_offset(PageSize::KiB16, 1) + 5];
    assert_eq!(
        u16::from_le_bytes([slot0_entry[0], slot0_entry[1]]),
        HEADER_SIZE as u16
    );
    assert_eq!(
        u16::from_le_bytes([slot1_entry[0], slot1_entry[1]]),
        (HEADER_SIZE + 3) as u16
    );
}

#[test]
fn test_heap_page_v1_golden_bytes_16kib_two_tuple_layout() {
    let page_size = PageSize::KiB16;
    let mut page =
        HeapPageInsert::new(PageId::new(8), page_size).expect("create heap page insert context");

    page.insert_raw_tuple(b"abc").expect("insert slot 0");
    page.insert_raw_tuple(b"defgh").expect("insert slot 1");

    let image = page.serialize().expect("serialize heap page");

    assert_eq!(&image[HEADER_SIZE..HEADER_SIZE + 8], b"abcdefgh");
    assert_eq!(
        &image[metadata_offset(page_size)..metadata_offset(page_size) + SLOT_METADATA_SIZE],
        &[0x02, 0x00, 0x78, 0x00],
        "footer metadata is [slot_count:LE=2][free_offset:LE=120]"
    );
    assert_eq!(
        &image[slot_offset(page_size, 0)..slot_offset(page_size, 0) + SLOT_ENTRY_SIZE],
        &[0x70, 0x00, 0x03, 0x00, 0x00],
        "slot 0 is offset 112, len 3, live"
    );
    assert_eq!(
        &image[slot_offset(page_size, 1)..slot_offset(page_size, 1) + SLOT_ENTRY_SIZE],
        &[0x73, 0x00, 0x05, 0x00, 0x00],
        "slot 1 is offset 115, len 5, live"
    );
    assert_eq!(
        metadata_offset(page_size) + SLOT_METADATA_SIZE,
        page_size.bytes_usize() - TRAILER_SIZE,
        "heap footer metadata ends exactly where PageTrailerV1 starts"
    );
}

#[test]
fn test_heap_page_v1_golden_bytes_32kib_slot_directory_placement() {
    let page_size = PageSize::KiB32;
    let mut page =
        HeapPageInsert::new(PageId::new(9), page_size).expect("create heap page insert context");

    page.insert_raw_tuple(b"z").expect("insert slot 0");

    let image = page.serialize().expect("serialize heap page");

    assert_eq!(metadata_offset(page_size), 32716);
    assert_eq!(slot_offset(page_size, 0), 32711);
    assert_eq!(&image[HEADER_SIZE..HEADER_SIZE + 1], b"z");
    assert_eq!(
        &image[metadata_offset(page_size)..metadata_offset(page_size) + SLOT_METADATA_SIZE],
        &[0x01, 0x00, 0x71, 0x00],
        "footer metadata is [slot_count:LE=1][free_offset:LE=113]"
    );
    assert_eq!(
        &image[slot_offset(page_size, 0)..slot_offset(page_size, 0) + SLOT_ENTRY_SIZE],
        &[0x70, 0x00, 0x01, 0x00, 0x00]
    );
}

#[test]
fn test_heap_page_from_image_reads_footer_metadata_layout() {
    let page_size = PageSize::KiB16;
    let image = heap_v1_image_with_slot0_tuple(page_size, b"abc");

    let page = HeapPage::from_image(page_size, &image).expect("footer metadata image is valid");
    assert_eq!(page.slot_count(), 1);
    assert_eq!(page.read_tuple(0).expect("read tuple"), b"abc");
}

#[test]
fn test_heap_page_v1_survives_disk_page_store_header_overlay() {
    let page_size = PageSize::KiB16;
    let page_id = PageId::new(17);
    let mut page =
        HeapPageInsert::new(page_id, page_size).expect("create heap page insert context");

    page.insert_raw_tuple(b"abc").expect("insert slot 0");
    page.insert_raw_tuple(b"defgh").expect("insert slot 1");
    let bytes = page.serialize().expect("serialize heap page");

    let payload_len = page_size.bytes_usize() - HEADER_SIZE - TRAILER_SIZE;
    let free_start = HEADER_SIZE + 8;
    let free_end = slot_base(page_size, 2);
    let header = PageHeader {
        magic: PageHeader::MAGIC,
        format_version: PageHeader::FORMAT_VERSION_V0,
        page_size,
        page_type: PageType::FixedRow,
        page_id,
        object_id: ObjectId::new(2),
        allocation_id: AllocationId::new(3),
        page_lsn: Lsn::new(10),
        page_epoch: 42,
        previous_page_id: None,
        next_page_id: Some(PageId::new(0x0000_0060_0000_0000)),
        header_len: HEADER_SIZE as u16,
        payload_offset: HEADER_SIZE as u32,
        payload_len: payload_len as u32,
        free_start: free_start as u32,
        free_end: free_end as u32,
        free_bytes: (free_end - free_start) as u32,
        slot_count: 2,
        row_count: 2,
        flags: PageFlags::HAS_NEXT,
        header_crc: 1,
    };
    let trailer =
        integrity_trailer_for_payload(&header, &bytes[HEADER_SIZE..HEADER_SIZE + payload_len]);
    let image = PageImage::with_layout(PageLayoutContract { header, trailer }, bytes)
        .expect("heap page image has a valid layout contract");

    let temp_dir = tempfile::TempDir::new().expect("temp dir");
    let data_file = temp_dir.path().join("heap-pages.bin");
    let temp_io_dir = temp_dir.path().join("io");
    let mut store = DiskPageStore::new(&data_file, &temp_io_dir).expect("disk page store");
    store
        .allocate_extent(ExtentDescriptor {
            extent_id: ExtentId::new(1),
            object_id: ObjectId::new(2),
            allocation_id: AllocationId::new(3),
            first_page_id: page_id,
            page_count: 1,
            page_size,
            state: ExtentState::AllocatingHot,
            segment_id: None,
            file_offset: 0,
            allocated_on_disk: false,
        })
        .expect("allocate extent");
    store
        .write_page(image, Lsn::new(10))
        .expect("write heap page through DiskPageStore");

    let persisted = store
        .read_page(page_id)
        .expect("read heap page")
        .expect("page exists");
    assert_eq!(
        &persisted.as_bytes()[HEADER_SIZE..HEADER_SIZE + 8],
        b"abcdefgh"
    );

    let heap = HeapPage::from_image(page_size, persisted.as_bytes())
        .expect("persisted heap image remains readable");
    assert_eq!(heap.read_tuple(0).expect("read slot 0"), b"abc");
    assert_eq!(heap.read_tuple(1).expect("read slot 1"), b"defgh");
}

#[test]
fn test_heap_page_v1_accepts_page_codec_v1_header_slot_count_guard() {
    let page_size = PageSize::KiB16;
    let mut image = heap_v1_image_with_slot0_tuple(page_size, b"abc");

    let payload_len = page_size.bytes_usize() - HEADER_SIZE - TRAILER_SIZE;
    let free_end = slot_base(page_size, 1);
    let header = PageHeader {
        magic: PageHeader::MAGIC,
        format_version: PageHeader::FORMAT_VERSION_V0,
        page_size,
        page_type: PageType::FixedRow,
        page_id: PageId::new(18),
        object_id: ObjectId::new(2),
        allocation_id: AllocationId::new(3),
        page_lsn: Lsn::new(11),
        page_epoch: 99,
        previous_page_id: None,
        next_page_id: Some(PageId::new(0x0000_0060_0000_0000)),
        header_len: HEADER_SIZE as u16,
        payload_offset: HEADER_SIZE as u32,
        payload_len: payload_len as u32,
        free_start: (HEADER_SIZE + 3) as u32,
        free_end: free_end as u32,
        free_bytes: (free_end - (HEADER_SIZE + 3)) as u32,
        slot_count: 1,
        row_count: 1,
        flags: PageFlags::HAS_NEXT,
        header_crc: 1,
    };
    let encoded_header = PageCodecV1::encode_header(&header).expect("encode PageCodecV1 header");
    image[..HEADER_SIZE].copy_from_slice(&encoded_header);

    let page = HeapPage::from_image(page_size, &image)
        .expect("PageCodecV1 header slot count guard accepts matching footer");
    assert_eq!(page.read_tuple(0).expect("read tuple"), b"abc");
}
