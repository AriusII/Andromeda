use andromeda_storage_heap::{SlotEntry, slot_directory::SlotDirectory};
use andromeda_storage_page::{PageHeader, PageSize};

use crate::support::{
    HEADER_SIZE, HEADER_SLOT_COUNT_OFFSET, assert_heap_image_rejected, empty_heap_v1_image,
    heap_v1_image_with_slot0_tuple, write_heap_v1_slot, write_heap_v1_slot_bytes,
    write_heap_v1_slot_metadata,
};

#[test]
fn test_heap_page_from_image_rejects_footer_slot_count_over_limit() {
    let page_size = PageSize::KiB16;
    let mut image = empty_heap_v1_image(page_size);
    write_heap_v1_slot_metadata(&mut image, page_size, 257, HEADER_SIZE as u16);

    assert_heap_image_rejected(page_size, &image, "slot count 257 exceeds limit 256");
}

#[test]
fn test_heap_page_from_image_rejects_footer_free_offset_before_live_tuple_end() {
    let page_size = PageSize::KiB16;
    let mut image = empty_heap_v1_image(page_size);
    image[HEADER_SIZE..HEADER_SIZE + 3].copy_from_slice(b"abc");
    write_heap_v1_slot(&mut image, page_size, 0, HEADER_SIZE as u16, 3);
    write_heap_v1_slot_metadata(&mut image, page_size, 1, (HEADER_SIZE + 2) as u16);

    assert_heap_image_rejected(page_size, &image, "exceeds footer free offset");
}

#[test]
fn test_heap_page_from_image_rejects_live_slot_before_header() {
    let page_size = PageSize::KiB16;
    let mut image = empty_heap_v1_image(page_size);
    write_heap_v1_slot(&mut image, page_size, 0, (HEADER_SIZE - 1) as u16, 1);
    write_heap_v1_slot_metadata(&mut image, page_size, 1, HEADER_SIZE as u16);

    assert_heap_image_rejected(page_size, &image, "outside payload region");
}

#[test]
fn test_heap_page_from_image_rejects_slot_tuple_overlap() {
    let page_size = PageSize::KiB16;
    let mut image = empty_heap_v1_image(page_size);
    image[HEADER_SIZE..HEADER_SIZE + 6].copy_from_slice(b"abcdef");
    write_heap_v1_slot(&mut image, page_size, 0, HEADER_SIZE as u16, 4);
    write_heap_v1_slot(&mut image, page_size, 1, (HEADER_SIZE + 2) as u16, 4);
    write_heap_v1_slot_metadata(&mut image, page_size, 2, (HEADER_SIZE + 6) as u16);

    assert_heap_image_rejected(page_size, &image, "tuple overlap");
}

#[test]
fn test_heap_page_from_image_rejects_deleted_slot_with_nonzero_offset() {
    let page_size = PageSize::KiB16;
    let mut image = empty_heap_v1_image(page_size);
    let mut slot = SlotEntry::new(HEADER_SIZE as u16, 3).to_bytes();
    slot[4] = 0x01;
    write_heap_v1_slot_bytes(&mut image, page_size, 0, slot);
    write_heap_v1_slot_metadata(&mut image, page_size, 1, HEADER_SIZE as u16);

    assert_heap_image_rejected(page_size, &image, "deleted flag requires zero offset");
}

#[test]
fn test_heap_page_from_image_rejects_unknown_slot_flags() {
    let page_size = PageSize::KiB16;
    let mut image = empty_heap_v1_image(page_size);
    let mut slot = SlotEntry::new(HEADER_SIZE as u16, 3).to_bytes();
    slot[4] = 0x02;
    write_heap_v1_slot_bytes(&mut image, page_size, 0, slot);
    write_heap_v1_slot_metadata(&mut image, page_size, 1, (HEADER_SIZE + 3) as u16);

    assert_heap_image_rejected(page_size, &image, "unknown flags");
}

#[test]
fn test_heap_page_from_image_rejects_header_footer_slot_count_mismatch() {
    let page_size = PageSize::KiB16;
    let mut image = heap_v1_image_with_slot0_tuple(page_size, b"abc");
    image[HEADER_SLOT_COUNT_OFFSET..HEADER_SLOT_COUNT_OFFSET + 2]
        .copy_from_slice(&2u16.to_le_bytes());

    assert_heap_image_rejected(page_size, &image, "slot count ambiguity");
}

#[test]
fn test_heap_page_from_image_rejects_persisted_header_version_mismatch() {
    let page_size = PageSize::KiB16;
    let mut image = empty_heap_v1_image(page_size);
    image[0..4].copy_from_slice(&PageHeader::MAGIC.to_le_bytes());
    image[4..6].copy_from_slice(&2u16.to_le_bytes());
    write_heap_v1_slot_metadata(&mut image, page_size, 0, 0);

    assert_heap_image_rejected(page_size, &image, "format version");
}

#[test]
fn test_heap_page_from_image_rejects_ambiguous_page_codec_header_lengths() {
    let page_size = PageSize::KiB16;
    let mut image = empty_heap_v1_image(page_size);
    image[0..4].copy_from_slice(&PageHeader::MAGIC.to_le_bytes());
    image[4..6].copy_from_slice(&PageHeader::FORMAT_VERSION_V0.to_le_bytes());
    image[68..70].copy_from_slice(&(HEADER_SIZE as u16).to_le_bytes());
    image[72..76].copy_from_slice(&((HEADER_SIZE as u32) + 1).to_le_bytes());
    write_heap_v1_slot_metadata(&mut image, page_size, 0, 0);

    assert_heap_image_rejected(page_size, &image, "payload offset");
}

#[test]
fn test_heap_page_from_image_rejects_legacy_header_only_slot_count() {
    let page_size = PageSize::KiB16;
    let mut image = empty_heap_v1_image(page_size);
    image[HEADER_SLOT_COUNT_OFFSET..HEADER_SLOT_COUNT_OFFSET + 2]
        .copy_from_slice(&1u16.to_le_bytes());

    assert_heap_image_rejected(page_size, &image, "slot count ambiguity");
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
