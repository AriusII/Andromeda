use andromeda_storage::{HEAP_PAGE_V1_PAYLOAD_OFFSET, HeapPage, PageSize, SlotEntry};

pub(crate) const HEADER_SIZE: usize = HEAP_PAGE_V1_PAYLOAD_OFFSET;
pub(crate) const TRAILER_SIZE: usize = 48;
pub(crate) const SLOT_ENTRY_SIZE: usize = 5;
pub(crate) const SLOT_METADATA_SIZE: usize = 4;
pub(crate) const HEADER_SLOT_COUNT_OFFSET: usize = 40;

pub(crate) fn metadata_offset(page_size: PageSize) -> usize {
    page_size.bytes_usize() - TRAILER_SIZE - SLOT_METADATA_SIZE
}

pub(crate) fn slot_offset(page_size: PageSize, slot_id: usize) -> usize {
    metadata_offset(page_size) - ((slot_id + 1) * SLOT_ENTRY_SIZE)
}

pub(crate) fn slot_base(page_size: PageSize, slot_count: usize) -> usize {
    metadata_offset(page_size) - (slot_count * SLOT_ENTRY_SIZE)
}

pub(crate) fn write_heap_v1_slot_metadata(
    image: &mut [u8],
    page_size: PageSize,
    slot_count: u16,
    free_offset: u16,
) {
    let meta = metadata_offset(page_size);
    image[meta..meta + 2].copy_from_slice(&slot_count.to_le_bytes());
    image[meta + 2..meta + 4].copy_from_slice(&free_offset.to_le_bytes());
}

pub(crate) fn write_heap_v1_slot(
    image: &mut [u8],
    page_size: PageSize,
    slot_id: usize,
    offset: u16,
    length: u16,
) {
    let slot = SlotEntry::new(offset, length).to_bytes();
    write_heap_v1_slot_bytes(image, page_size, slot_id, slot);
}

pub(crate) fn write_heap_v1_slot_bytes(
    image: &mut [u8],
    page_size: PageSize,
    slot_id: usize,
    slot: [u8; SLOT_ENTRY_SIZE],
) {
    let offset = slot_offset(page_size, slot_id);
    image[offset..offset + SLOT_ENTRY_SIZE].copy_from_slice(&slot);
}

pub(crate) fn empty_heap_v1_image(page_size: PageSize) -> Vec<u8> {
    vec![0u8; page_size.bytes_usize()]
}

pub(crate) fn heap_v1_image_with_slot0_tuple(page_size: PageSize, tuple: &[u8]) -> Vec<u8> {
    let mut image = empty_heap_v1_image(page_size);
    image[HEADER_SIZE..HEADER_SIZE + tuple.len()].copy_from_slice(tuple);
    write_heap_v1_slot(
        &mut image,
        page_size,
        0,
        HEADER_SIZE as u16,
        tuple.len() as u16,
    );
    write_heap_v1_slot_metadata(&mut image, page_size, 1, (HEADER_SIZE + tuple.len()) as u16);
    image
}

pub(crate) fn assert_heap_image_rejected(
    page_size: PageSize,
    image: &[u8],
    expected_message: &str,
) {
    let err = HeapPage::from_image(page_size, image).expect_err("heap image should be rejected");
    assert!(
        err.message().contains(expected_message),
        "unexpected error: {}",
        err.message()
    );
}
