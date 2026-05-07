#![forbid(unsafe_code)]

use andromeda_storage::slot_directory::{SlotDirectory, SlotId};
use andromeda_storage::{
    AllocationId, DiskPageStore, ExtentDescriptor, ExtentId, ExtentState,
    HEAP_PAGE_V1_PAYLOAD_OFFSET, HeapPage, HeapPageInsert, Lsn, ObjectId, PAGE_CODEC_V1_HEADER_LEN,
    PageCodecV1, PageFlags, PageHeader, PageId, PageImage, PageLayoutContract, PageSize, PageStore,
    PageType, SlotEntry, integrity_trailer_for_payload,
};

const HEADER_SIZE: usize = HEAP_PAGE_V1_PAYLOAD_OFFSET;
const TRAILER_SIZE: usize = 48;
const SLOT_ENTRY_SIZE: usize = 5;
const SLOT_METADATA_SIZE: usize = 4;
const HEADER_SLOT_COUNT_OFFSET: usize = 40;

fn metadata_offset(page_size: PageSize) -> usize {
    page_size.bytes_usize() - TRAILER_SIZE - SLOT_METADATA_SIZE
}

fn slot_offset(page_size: PageSize, slot_id: usize) -> usize {
    metadata_offset(page_size) - ((slot_id + 1) * SLOT_ENTRY_SIZE)
}

fn slot_base(page_size: PageSize, slot_count: usize) -> usize {
    metadata_offset(page_size) - (slot_count * SLOT_ENTRY_SIZE)
}

fn write_heap_v1_slot_metadata(
    image: &mut [u8],
    page_size: PageSize,
    slot_count: u16,
    free_offset: u16,
) {
    let meta = metadata_offset(page_size);
    image[meta..meta + 2].copy_from_slice(&slot_count.to_le_bytes());
    image[meta + 2..meta + 4].copy_from_slice(&free_offset.to_le_bytes());
}

fn write_heap_v1_slot(
    image: &mut [u8],
    page_size: PageSize,
    slot_id: usize,
    offset: u16,
    length: u16,
) {
    let slot = SlotEntry::new(offset, length).to_bytes();
    write_heap_v1_slot_bytes(image, page_size, slot_id, slot);
}

fn write_heap_v1_slot_bytes(
    image: &mut [u8],
    page_size: PageSize,
    slot_id: usize,
    slot: [u8; SLOT_ENTRY_SIZE],
) {
    let offset = slot_offset(page_size, slot_id);
    image[offset..offset + SLOT_ENTRY_SIZE].copy_from_slice(&slot);
}

// This test suite validates the heap slot directory implementation with
// comprehensive test coverage including:
// - Basic allocation and retrieval
// - Deletion and compaction
// - Free space management
// - Boundary conditions
// - Page size variants
// - Fragmentation patterns

#[test]
fn test_slot_directory_create_empty() {
    let dir = SlotDirectory::new(PageSize::KiB16);
    assert_eq!(dir.slot_count(), 0);
    assert_eq!(dir.active_slot_count(), 0);
    assert!(dir.free_space() > 0);
}

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
    let mut page = HeapPageInsert::new(andromeda_storage::PageId::new(7), PageSize::KiB16)
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
    let mut page = HeapPageInsert::new(andromeda_storage::PageId::new(8), page_size)
        .expect("create heap page insert context");

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
    let mut page = HeapPageInsert::new(andromeda_storage::PageId::new(9), page_size)
        .expect("create heap page insert context");

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
    let mut image = vec![0u8; page_size.bytes_usize()];
    image[HEADER_SIZE..HEADER_SIZE + 3].copy_from_slice(b"abc");
    write_heap_v1_slot(&mut image, page_size, 0, HEADER_SIZE as u16, 3);
    write_heap_v1_slot_metadata(&mut image, page_size, 1, (HEADER_SIZE + 3) as u16);

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
    let mut image = vec![0u8; page_size.bytes_usize()];
    image[HEADER_SIZE..HEADER_SIZE + 3].copy_from_slice(b"abc");
    write_heap_v1_slot(&mut image, page_size, 0, HEADER_SIZE as u16, 3);
    write_heap_v1_slot_metadata(&mut image, page_size, 1, (HEADER_SIZE + 3) as u16);

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

#[test]
fn test_heap_page_from_image_rejects_footer_slot_count_over_limit() {
    let page_size = PageSize::KiB16;
    let mut image = vec![0u8; page_size.bytes_usize()];
    write_heap_v1_slot_metadata(&mut image, page_size, 257, HEADER_SIZE as u16);

    let err = HeapPage::from_image(page_size, &image).expect_err("slot count limit rejected");
    assert!(
        err.message().contains("slot count 257 exceeds limit 256"),
        "unexpected error: {}",
        err.message()
    );
}

#[test]
fn test_heap_page_from_image_rejects_footer_free_offset_before_live_tuple_end() {
    let page_size = PageSize::KiB16;
    let mut image = vec![0u8; page_size.bytes_usize()];
    image[HEADER_SIZE..HEADER_SIZE + 3].copy_from_slice(b"abc");
    write_heap_v1_slot(&mut image, page_size, 0, HEADER_SIZE as u16, 3);
    write_heap_v1_slot_metadata(&mut image, page_size, 1, (HEADER_SIZE + 2) as u16);

    let err = HeapPage::from_image(page_size, &image).expect_err("free offset bounds rejected");
    assert!(
        err.message().contains("exceeds footer free offset"),
        "unexpected error: {}",
        err.message()
    );
}

#[test]
fn test_heap_page_from_image_rejects_live_slot_before_header() {
    let page_size = PageSize::KiB16;
    let mut image = vec![0u8; page_size.bytes_usize()];
    write_heap_v1_slot(&mut image, page_size, 0, (HEADER_SIZE - 1) as u16, 1);
    write_heap_v1_slot_metadata(&mut image, page_size, 1, HEADER_SIZE as u16);

    let err = HeapPage::from_image(page_size, &image).expect_err("header overlap rejected");
    assert!(
        err.message().contains("outside payload region"),
        "unexpected error: {}",
        err.message()
    );
}

#[test]
fn test_heap_page_from_image_rejects_slot_tuple_overlap() {
    let page_size = PageSize::KiB16;
    let mut image = vec![0u8; page_size.bytes_usize()];
    image[HEADER_SIZE..HEADER_SIZE + 6].copy_from_slice(b"abcdef");
    write_heap_v1_slot(&mut image, page_size, 0, HEADER_SIZE as u16, 4);
    write_heap_v1_slot(&mut image, page_size, 1, (HEADER_SIZE + 2) as u16, 4);
    write_heap_v1_slot_metadata(&mut image, page_size, 2, (HEADER_SIZE + 6) as u16);

    let err = HeapPage::from_image(page_size, &image).expect_err("overlap rejected");
    assert!(
        err.message().contains("tuple overlap"),
        "unexpected error: {}",
        err.message()
    );
}

#[test]
fn test_heap_page_from_image_rejects_deleted_slot_with_nonzero_offset() {
    let page_size = PageSize::KiB16;
    let mut image = vec![0u8; page_size.bytes_usize()];
    let mut slot = SlotEntry::new(HEADER_SIZE as u16, 3).to_bytes();
    slot[4] = 0x01;
    write_heap_v1_slot_bytes(&mut image, page_size, 0, slot);
    write_heap_v1_slot_metadata(&mut image, page_size, 1, HEADER_SIZE as u16);

    let err = HeapPage::from_image(page_size, &image).expect_err("deleted offset rejected");
    assert!(
        err.message().contains("deleted flag requires zero offset"),
        "unexpected error: {}",
        err.message()
    );
}

#[test]
fn test_heap_page_from_image_rejects_unknown_slot_flags() {
    let page_size = PageSize::KiB16;
    let mut image = vec![0u8; page_size.bytes_usize()];
    let mut slot = SlotEntry::new(HEADER_SIZE as u16, 3).to_bytes();
    slot[4] = 0x02;
    write_heap_v1_slot_bytes(&mut image, page_size, 0, slot);
    write_heap_v1_slot_metadata(&mut image, page_size, 1, (HEADER_SIZE + 3) as u16);

    let err = HeapPage::from_image(page_size, &image).expect_err("unknown flags rejected");
    assert!(
        err.message().contains("unknown flags"),
        "unexpected error: {}",
        err.message()
    );
}

#[test]
fn test_heap_page_from_image_rejects_header_footer_slot_count_mismatch() {
    let page_size = PageSize::KiB16;
    let mut image = vec![0u8; page_size.bytes_usize()];
    image[HEADER_SLOT_COUNT_OFFSET..HEADER_SLOT_COUNT_OFFSET + 2]
        .copy_from_slice(&2u16.to_le_bytes());
    write_heap_v1_slot(&mut image, page_size, 0, HEADER_SIZE as u16, 3);
    write_heap_v1_slot_metadata(&mut image, page_size, 1, (HEADER_SIZE + 3) as u16);

    let err = HeapPage::from_image(page_size, &image).expect_err("mismatched counts rejected");
    assert!(
        err.message().contains("slot count ambiguity"),
        "unexpected error: {}",
        err.message()
    );
}

#[test]
fn test_heap_page_from_image_rejects_persisted_header_version_mismatch() {
    let page_size = PageSize::KiB16;
    let mut image = vec![0u8; page_size.bytes_usize()];
    image[0..4].copy_from_slice(&PageHeader::MAGIC.to_le_bytes());
    image[4..6].copy_from_slice(&2u16.to_le_bytes());
    write_heap_v1_slot_metadata(&mut image, page_size, 0, 0);

    let err = HeapPage::from_image(page_size, &image).expect_err("bad version rejected");
    assert!(
        err.message().contains("format version"),
        "unexpected error: {}",
        err.message()
    );
}

#[test]
fn test_heap_page_from_image_rejects_ambiguous_page_codec_header_lengths() {
    let page_size = PageSize::KiB16;
    let mut image = vec![0u8; page_size.bytes_usize()];
    image[0..4].copy_from_slice(&PageHeader::MAGIC.to_le_bytes());
    image[4..6].copy_from_slice(&PageHeader::FORMAT_VERSION_V0.to_le_bytes());
    image[68..70].copy_from_slice(&(HEADER_SIZE as u16).to_le_bytes());
    image[72..76].copy_from_slice(&((HEADER_SIZE as u32) + 1).to_le_bytes());
    write_heap_v1_slot_metadata(&mut image, page_size, 0, 0);

    let err = HeapPage::from_image(page_size, &image).expect_err("bad payload offset rejected");
    assert!(
        err.message().contains("payload offset"),
        "unexpected error: {}",
        err.message()
    );
}

#[test]
fn test_heap_page_from_image_rejects_legacy_header_only_slot_count() {
    let page_size = PageSize::KiB16;
    let mut image = vec![0u8; page_size.bytes_usize()];
    image[HEADER_SLOT_COUNT_OFFSET..HEADER_SLOT_COUNT_OFFSET + 2]
        .copy_from_slice(&1u16.to_le_bytes());

    let err =
        HeapPage::from_image(page_size, &image).expect_err("legacy header-only page rejected");
    assert!(
        err.message().contains("slot count ambiguity"),
        "unexpected error: {}",
        err.message()
    );
}

#[test]
fn test_slot_directory_rejects_header_footer_slot_count_mismatch() {
    let page_size = PageSize::KiB16;
    let mut image = vec![0u8; page_size.bytes_usize()];
    image[HEADER_SLOT_COUNT_OFFSET..HEADER_SLOT_COUNT_OFFSET + 2]
        .copy_from_slice(&2u16.to_le_bytes());
    write_heap_v1_slot(&mut image, page_size, 0, HEADER_SIZE as u16, 3);
    write_heap_v1_slot_metadata(&mut image, page_size, 1, (HEADER_SIZE + 3) as u16);

    let err =
        SlotDirectory::from_page_data(page_size, &image).expect_err("mismatched counts rejected");
    assert!(
        err.message().contains("slot count ambiguity"),
        "unexpected error: {}",
        err.message()
    );
}

#[test]
fn test_slot_directory_rejects_legacy_header_only_slot_count() {
    let page_size = PageSize::KiB16;
    let mut image = vec![0u8; page_size.bytes_usize()];
    image[HEADER_SLOT_COUNT_OFFSET..HEADER_SLOT_COUNT_OFFSET + 2]
        .copy_from_slice(&1u16.to_le_bytes());

    let err = SlotDirectory::from_page_data(page_size, &image)
        .expect_err("legacy header-only page rejected");
    assert!(
        err.message().contains("slot count ambiguity"),
        "unexpected error: {}",
        err.message()
    );
}

#[test]
fn test_slot_directory_allocate_single() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let slot_id = dir.allocate_slot(100).expect("allocation should succeed");

    assert_eq!(slot_id.get(), 0);
    assert_eq!(dir.slot_count(), 1);
    assert_eq!(dir.active_slot_count(), 1);

    let (offset, length) = dir
        .get_slot(slot_id)
        .expect("get_slot")
        .expect("slot exists");
    assert_eq!(offset, HEADER_SIZE as u16);
    assert_eq!(length, 100);
}

#[test]
fn test_slot_directory_allocate_sequence() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let slot1 = dir.allocate_slot(100).expect("alloc 1");
    let slot2 = dir.allocate_slot(200).expect("alloc 2");
    let slot3 = dir.allocate_slot(150).expect("alloc 3");

    assert_eq!(dir.slot_count(), 3);
    assert_eq!(dir.active_slot_count(), 3);

    let (o1, l1) = dir.get_slot(slot1).expect("get 1").expect("slot 1");
    assert_eq!((o1, l1), (HEADER_SIZE as u16, 100));

    let (o2, l2) = dir.get_slot(slot2).expect("get 2").expect("slot 2");
    assert_eq!((o2, l2), ((HEADER_SIZE + 100) as u16, 200));

    let (o3, l3) = dir.get_slot(slot3).expect("get 3").expect("slot 3");
    assert_eq!((o3, l3), ((HEADER_SIZE + 300) as u16, 150));
}

#[test]
fn test_slot_directory_get_nonexistent() {
    let dir = SlotDirectory::new(PageSize::KiB16);
    let result = dir.get_slot(SlotId::new(0)).expect("get succeeds");
    assert!(result.is_none());
}

#[test]
fn test_slot_directory_mark_deleted() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let slot = dir.allocate_slot(100).expect("alloc");

    assert_eq!(dir.active_slot_count(), 1);

    dir.mark_deleted(slot).expect("mark_deleted succeeds");

    assert_eq!(dir.slot_count(), 1); // Slot still exists
    assert_eq!(dir.active_slot_count(), 0); // But not active
    assert!(dir.get_slot(slot).expect("get_slot").is_none()); // Returns None
}

#[test]
fn test_slot_directory_double_delete_error() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let slot = dir.allocate_slot(100).expect("alloc");

    dir.mark_deleted(slot).expect("first delete");

    let result = dir.mark_deleted(slot);
    assert!(result.is_err(), "second delete should fail");
}

#[test]
fn test_slot_directory_compact_removes_deleted_tail() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let _slot1 = dir.allocate_slot(100).expect("alloc 1");
    let _slot2 = dir.allocate_slot(200).expect("alloc 2");
    let slot3 = dir.allocate_slot(150).expect("alloc 3");

    assert_eq!(dir.slot_count(), 3);

    dir.mark_deleted(slot3).expect("delete 3");
    assert_eq!(dir.slot_count(), 3);

    let freed = dir.compact();

    assert_eq!(freed, 150, "should free 150 bytes");
    assert_eq!(dir.slot_count(), 2, "should remove tail deleted slot");
    assert_eq!(dir.active_slot_count(), 2);

    // slot3 should no longer exist
    assert!(dir.get_slot(slot3).expect("get_slot").is_none());
}

#[test]
fn test_slot_directory_compact_with_gaps() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let _slot1 = dir.allocate_slot(100).expect("alloc 1");
    let slot2 = dir.allocate_slot(200).expect("alloc 2");
    let _slot3 = dir.allocate_slot(150).expect("alloc 3");
    let slot4 = dir.allocate_slot(75).expect("alloc 4");

    // Delete middle slot (slot2)
    dir.mark_deleted(slot2).expect("delete 2");
    assert_eq!(dir.active_slot_count(), 3);

    // Compact only removes deleted TAIL slots
    let freed = dir.compact();

    // Should free 0 because slot2 is not at the tail
    assert_eq!(freed, 0);
    assert_eq!(dir.slot_count(), 4, "all slots still present");

    // Now delete tail
    dir.mark_deleted(slot4).expect("delete 4");
    let freed2 = dir.compact();
    assert_eq!(freed2, 75);
    assert_eq!(dir.slot_count(), 3, "should remove tail slot 4");
}

#[test]
fn test_slot_directory_free_space() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let initial_free = dir.free_space();
    assert!(initial_free > 0, "should have initial free space");

    dir.allocate_slot(1000).expect("alloc 1000");
    let after_alloc = dir.free_space();

    assert!(after_alloc < initial_free, "free space should decrease");
    let used = initial_free.saturating_sub(after_alloc);
    assert_eq!(
        used, 1005,
        "should account for tuple bytes and slot metadata"
    );
}

#[test]
fn test_slot_directory_allocate_zero_length_fails() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let result = dir.allocate_slot(0);
    assert!(result.is_err(), "zero-length allocation should fail");
}

#[test]
fn test_slot_directory_allocate_exceeds_capacity() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let excessive = 20000u16; // 20KB > 16KB page
    let result = dir.allocate_slot(excessive);
    assert!(result.is_err(), "excessive allocation should fail");
}

#[test]
fn test_slot_directory_mark_deleted_invalid_slot() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let result = dir.mark_deleted(SlotId::new(0));
    assert!(result.is_err(), "invalid slot ID should fail");
}

#[test]
fn test_slot_directory_get_slot_accuracy() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let slot = dir.allocate_slot(512).expect("alloc");

    let result = dir.get_slot(slot).expect("get_slot").expect("should exist");
    assert_eq!(
        result.0, HEADER_SIZE as u16,
        "offset should be after durable header reserve"
    );
    assert_eq!(result.1, 512, "length should match");
}

#[test]
fn test_slot_id_operations() {
    let slot_id = SlotId::new(42);
    assert_eq!(slot_id.get(), 42);
    assert_eq!(slot_id.as_usize(), 42usize);

    let slot_id2 = SlotId::new(42);
    assert_eq!(slot_id, slot_id2);

    let slot_id3 = SlotId::new(41);
    assert!(slot_id > slot_id3);
}

#[test]
fn test_slot_directory_validate_empty() {
    let dir = SlotDirectory::new(PageSize::KiB16);
    dir.validate().expect("empty directory should validate");
}

#[test]
fn test_slot_directory_validate_with_slots() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    dir.allocate_slot(100).expect("alloc");
    dir.allocate_slot(200).expect("alloc");
    dir.allocate_slot(150).expect("alloc");

    dir.validate().expect("should validate with no overlaps");
}

#[test]
fn test_slot_directory_page_size_32kb() {
    let mut dir = SlotDirectory::new(PageSize::KiB32);

    // 32KB page should support more space
    for i in 0..5 {
        let size = 1000u16 + (i * 100u16);
        let result = dir.allocate_slot(size);
        assert!(result.is_ok(), "allocation {} should succeed", i);
    }

    assert_eq!(dir.slot_count(), 5);
    assert_eq!(dir.active_slot_count(), 5);
}

#[test]
fn test_slot_directory_16kb_max_slots() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    // Should be able to allocate ~256 slots max (small sizes to fit)
    let mut count = 0;
    for _ in 0..300 {
        match dir.allocate_slot(10) {
            Ok(_) => count += 1,
            Err(_) => break, // Ran out of space
        }
    }

    assert!(count > 0, "should allocate some slots");
    assert!(count <= 256, "should not exceed max slots");
}

#[test]
fn test_slot_directory_serialize_deserialize() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let _slot1 = dir.allocate_slot(100).expect("alloc 1");
    let _slot2 = dir.allocate_slot(200).expect("alloc 2");

    // Create page buffer
    let mut page_data = vec![0u8; 16384]; // 16 KiB
    dir.serialize_to_page(&mut page_data).expect("serialize");

    // Deserialize and verify
    let restored = SlotDirectory::from_page_data(PageSize::KiB16, &page_data).expect("deserialize");

    assert_eq!(restored.slot_count(), dir.slot_count());
    assert_eq!(restored.active_slot_count(), dir.active_slot_count());
}

#[test]
fn test_slot_directory_reuse_deleted_slot() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let slot1 = dir.allocate_slot(100).expect("alloc 1");
    dir.allocate_slot(200).expect("alloc 2");

    assert_eq!(dir.slot_count(), 2);

    // Delete first slot
    dir.mark_deleted(slot1).expect("delete 1");
    assert_eq!(dir.active_slot_count(), 1);

    // Allocate new slot - should reuse slot1's ID
    let slot3 = dir.allocate_slot(150).expect("alloc 3");
    assert_eq!(slot3.get(), 0, "should reuse first slot ID");
    assert_eq!(dir.slot_count(), 2, "total slots unchanged");
    assert_eq!(dir.active_slot_count(), 2, "active slots restored");
}

#[test]
fn test_slot_directory_fragmentation_pattern() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    // Allocate pattern: large, small, large, small
    let large1 = dir.allocate_slot(500).expect("large 1");
    let small1 = dir.allocate_slot(50).expect("small 1");
    let large2 = dir.allocate_slot(500).expect("large 2");
    let small2 = dir.allocate_slot(50).expect("small 2");

    assert_eq!(dir.slot_count(), 4);

    // Delete alternating: delete small slots
    dir.mark_deleted(small1).expect("delete small 1");
    dir.mark_deleted(small2).expect("delete small 2");

    assert_eq!(dir.active_slot_count(), 2);

    // Compact removes only the deleted tail slot and leaves middle gaps intact.
    let freed = dir.compact();
    assert_eq!(freed, 50, "should remove only the deleted tail slot");

    // Verify large slots still exist
    assert!(dir.get_slot(large1).expect("get").is_some());
    assert!(dir.get_slot(large2).expect("get").is_some());
}

#[test]
fn test_slot_directory_no_tuple_overlap() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let slot1 = dir.allocate_slot(100).expect("alloc 1");
    let slot2 = dir.allocate_slot(150).expect("alloc 2");
    let slot3 = dir.allocate_slot(200).expect("alloc 3");

    let (o1, l1) = dir.get_slot(slot1).expect("get").expect("slot 1");
    let (o2, l2) = dir.get_slot(slot2).expect("get").expect("slot 2");
    let (o3, _l3) = dir.get_slot(slot3).expect("get").expect("slot 3");

    // Verify no overlap
    let end1 = o1 as u32 + l1 as u32;
    let end2 = o2 as u32 + l2 as u32;

    assert!(end1 <= o2 as u32, "slot 1 should not overlap slot 2");
    assert!(end2 <= o3 as u32, "slot 2 should not overlap slot 3");

    // Validate should succeed
    dir.validate().expect("should validate");
}

#[test]
fn test_slot_directory_active_count_accuracy() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    for _ in 0..10 {
        dir.allocate_slot(50).expect("alloc");
    }

    assert_eq!(dir.active_slot_count(), 10);

    // Delete 3 slots
    dir.mark_deleted(SlotId::new(0)).expect("delete");
    dir.mark_deleted(SlotId::new(2)).expect("delete");
    dir.mark_deleted(SlotId::new(5)).expect("delete");

    assert_eq!(dir.active_slot_count(), 7, "should have 7 active slots");

    // Compact won't remove these (not tail)
    dir.compact();
    assert_eq!(
        dir.active_slot_count(),
        7,
        "active count unchanged after compact"
    );
}

#[test]
fn test_slot_directory_large_tuple() {
    let mut dir = SlotDirectory::new(PageSize::KiB32); // Use 32KB page

    // Allocate 8KB tuple
    let slot = dir.allocate_slot(8192).expect("allocate 8KB");
    let (offset, length) = dir.get_slot(slot).expect("get").expect("exists");
    assert_eq!(length, 8192);
    assert!(offset > 0);
}

#[test]
fn test_slot_directory_header_boundary() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    // First allocation starts after the durable header reserve.
    let slot1 = dir.allocate_slot(100).expect("alloc");
    let (offset1, _length1) = dir.get_slot(slot1).expect("get").expect("exists");

    assert_eq!(
        offset1, HEADER_SIZE as u16,
        "first slot should start at the HeapPageV1 payload offset"
    );
}

#[test]
fn test_slot_directory_allocation_patterns() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    // Pattern: alternate between large and small
    let slots: Vec<_> = (0..10)
        .map(|i| {
            let size = if i % 2 == 0 { 200 } else { 50 };
            dir.allocate_slot(size).expect("alloc")
        })
        .collect();

    assert_eq!(dir.slot_count(), 10);
    assert_eq!(dir.active_slot_count(), 10);

    // Verify all are retrievable
    for slot_id in &slots {
        assert!(dir.get_slot(*slot_id).expect("get").is_some());
    }
}

#[test]
fn test_slot_directory_compact_all_deleted() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let slot1 = dir.allocate_slot(100).expect("alloc 1");
    let slot2 = dir.allocate_slot(200).expect("alloc 2");

    dir.mark_deleted(slot1).expect("delete 1");
    dir.mark_deleted(slot2).expect("delete 2");

    assert_eq!(dir.active_slot_count(), 0);

    let freed = dir.compact();
    assert_eq!(freed, 200, "should free slot 2");
    assert_eq!(dir.slot_count(), 1, "should have 1 slot remaining");

    let freed2 = dir.compact();
    assert_eq!(freed2, 100, "should free slot 1 in second compact");
    assert_eq!(dir.slot_count(), 0, "should be empty");
}

#[test]
fn test_slot_directory_free_space_monotonic() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let free1 = dir.free_space();
    dir.allocate_slot(100).expect("alloc 1");
    let free2 = dir.free_space();
    dir.allocate_slot(100).expect("alloc 2");
    let free3 = dir.free_space();
    dir.allocate_slot(100).expect("alloc 3");
    let free4 = dir.free_space();

    assert!(free1 > free2);
    assert!(free2 > free3);
    assert!(free3 > free4);
}

#[test]
fn test_slot_directory_page_size_capacity() {
    let mut dir16 = SlotDirectory::new(PageSize::KiB16);
    let mut dir32 = SlotDirectory::new(PageSize::KiB32);

    let test_size = 5000u16;

    let result16 = dir16.allocate_slot(test_size);
    let result32 = dir32.allocate_slot(test_size);

    // Both might succeed, but 32KB should have more remaining space
    let free16 = dir16.free_space();
    let free32 = dir32.free_space();

    // 32KB page should have significantly more free space than 16KB
    assert!(free32 > free16 || (result16.is_err() && result32.is_ok()));
}

#[test]
fn test_slot_directory_serialize_preserves_structure() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let slots: Vec<_> = (0..5)
        .map(|i| dir.allocate_slot(100 + (i * 50u16)).expect("alloc"))
        .collect();

    // Delete some
    dir.mark_deleted(slots[1]).expect("delete");
    dir.mark_deleted(slots[3]).expect("delete");

    // Serialize
    let mut page_data = vec![0u8; 16384];
    dir.serialize_to_page(&mut page_data).expect("serialize");

    // Deserialize
    let restored = SlotDirectory::from_page_data(PageSize::KiB16, &page_data).expect("deserialize");

    // Verify structure matches
    assert_eq!(restored.slot_count(), dir.slot_count());
    assert_eq!(restored.active_slot_count(), dir.active_slot_count());

    // Verify specific slots
    assert!(restored.get_slot(slots[0]).expect("get").is_some());
    assert!(restored.get_slot(slots[1]).expect("get").is_none()); // deleted
    assert!(restored.get_slot(slots[2]).expect("get").is_some());
    assert!(restored.get_slot(slots[3]).expect("get").is_none()); // deleted
    assert!(restored.get_slot(slots[4]).expect("get").is_some());
}
