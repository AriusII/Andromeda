use andromeda_core::AndromedaResult;

use crate::PageSize;

use super::SlotEntry;
use super::heap_error;

pub(crate) const HEAP_PAGE_V1_HEADER_SIZE: usize = 96;
pub(crate) const HEAP_PAGE_V1_TRAILER_SIZE: usize = 48;
pub(crate) const HEAP_PAGE_V1_SLOT_METADATA_SIZE: usize = 4;
pub(crate) const HEAP_PAGE_V1_HEADER_SLOT_COUNT_OFFSET: usize = 40;
pub(crate) const HEAP_PAGE_V1_SLOT_FLAGS_KNOWN_MASK: u8 = 0x01;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HeapPageV1SlotMetadata {
    pub(crate) slot_count: usize,
    pub(crate) free_offset: u16,
    pub(crate) metadata_offset: usize,
    pub(crate) slot_base: usize,
}

pub(crate) fn heap_page_v1_max_slots(page_size: PageSize) -> usize {
    match page_size {
        PageSize::KiB16 => 256,
        PageSize::KiB32 => 512,
    }
}

pub(crate) fn heap_page_v1_metadata_offset(page_size: PageSize) -> usize {
    page_size.bytes_usize() - HEAP_PAGE_V1_TRAILER_SIZE - HEAP_PAGE_V1_SLOT_METADATA_SIZE
}

pub(crate) fn heap_page_v1_read_slot_metadata(
    page_size: PageSize,
    page_data: &[u8],
) -> AndromedaResult<HeapPageV1SlotMetadata> {
    if page_data.len() != page_size.bytes_usize() {
        return Err(heap_error(format!(
            "page size mismatch: expected {} bytes, got {}",
            page_size.bytes_usize(),
            page_data.len()
        )));
    }

    let metadata_offset = heap_page_v1_metadata_offset(page_size);
    if metadata_offset < HEAP_PAGE_V1_HEADER_SIZE {
        return Err(heap_error("page too small for heap page v1 metadata"));
    }

    let footer_slot_count =
        u16::from_le_bytes([page_data[metadata_offset], page_data[metadata_offset + 1]]) as usize;
    let free_offset = u16::from_le_bytes([
        page_data[metadata_offset + 2],
        page_data[metadata_offset + 3],
    ]);
    let max_slots = heap_page_v1_max_slots(page_size);
    if footer_slot_count > max_slots {
        return Err(heap_error(format!(
            "heap page v1 slot count {} exceeds limit {}",
            footer_slot_count, max_slots
        )));
    }

    // DEC-032 blocks legacy/header-only authority; header slot count must be 0 or
    // match footer metadata.
    let header_slot_count = u16::from_le_bytes([
        page_data[HEAP_PAGE_V1_HEADER_SLOT_COUNT_OFFSET],
        page_data[HEAP_PAGE_V1_HEADER_SLOT_COUNT_OFFSET + 1],
    ]) as usize;
    if header_slot_count != 0 && header_slot_count != footer_slot_count {
        return Err(heap_error(format!(
            "heap page v1 slot count ambiguity: header slot count {} does not match footer slot count {}",
            header_slot_count, footer_slot_count
        )));
    }

    let slot_directory_size = footer_slot_count
        .checked_mul(SlotEntry::SIZE)
        .ok_or_else(|| heap_error("heap page v1 slot directory size overflow"))?;
    if slot_directory_size > metadata_offset.saturating_sub(HEAP_PAGE_V1_HEADER_SIZE) {
        return Err(heap_error("slot directory overlaps page header"));
    }

    let slot_base = metadata_offset - slot_directory_size;
    let free_offset_usize = free_offset as usize;
    if footer_slot_count > 0 && free_offset == 0 {
        return Err(heap_error(
            "heap page v1 free offset missing for non-empty slot directory",
        ));
    }
    if free_offset != 0
        && (free_offset_usize < HEAP_PAGE_V1_HEADER_SIZE || free_offset_usize > slot_base)
    {
        return Err(heap_error(format!(
            "heap page v1 free offset {} outside payload/free-space bounds {}..={}",
            free_offset, HEAP_PAGE_V1_HEADER_SIZE, slot_base
        )));
    }

    Ok(HeapPageV1SlotMetadata {
        slot_count: footer_slot_count,
        free_offset,
        metadata_offset,
        slot_base,
    })
}
