use andromeda_core::AndromedaResult;

use super::{
    HEAP_PAGE_V1_HEADER_SIZE, HEAP_PAGE_V1_SLOT_FLAGS_KNOWN_MASK, HeapPageV1SlotMetadata,
    SlotEntry, heap_error,
};

pub(crate) fn heap_page_v1_read_and_validate_slots(
    page_data: &[u8],
    metadata: HeapPageV1SlotMetadata,
) -> AndromedaResult<Vec<SlotEntry>> {
    let mut slots = Vec::with_capacity(metadata.slot_count);
    let mut live_ranges: Vec<(usize, usize, usize)> = Vec::new();
    let free_offset = metadata.free_offset as usize;

    for i in 0..metadata.slot_count {
        let slot_offset = metadata.metadata_offset - ((i + 1) * SlotEntry::SIZE);
        let mut slot_bytes = [0u8; 5];
        slot_bytes.copy_from_slice(&page_data[slot_offset..slot_offset + 5]);
        let entry = SlotEntry::from_bytes(slot_bytes);

        if entry.flags & !HEAP_PAGE_V1_SLOT_FLAGS_KNOWN_MASK != 0 {
            return Err(heap_error(format!(
                "heap page v1 slot {} has unknown flags 0x{:02x}",
                i, entry.flags
            )));
        }

        if entry.is_deleted() {
            if entry.offset != 0 {
                return Err(heap_error(format!(
                    "heap page v1 slot {} deleted flag requires zero offset",
                    i
                )));
            }
            slots.push(entry);
            continue;
        }

        if entry.offset == 0 || entry.length == 0 {
            return Err(heap_error(format!(
                "heap page v1 live slot {} requires non-zero offset and length",
                i
            )));
        }

        let tuple_start = entry.offset as usize;
        let tuple_end = tuple_start
            .checked_add(entry.length as usize)
            .ok_or_else(|| heap_error(format!("heap page v1 slot {} tuple bounds overflow", i)))?;
        if tuple_start < HEAP_PAGE_V1_HEADER_SIZE || tuple_end > metadata.slot_base {
            return Err(heap_error(format!(
                "heap page v1 slot {} tuple bounds {}..{} outside payload region {}..{}",
                i, tuple_start, tuple_end, HEAP_PAGE_V1_HEADER_SIZE, metadata.slot_base
            )));
        }
        if tuple_end > free_offset {
            return Err(heap_error(format!(
                "heap page v1 slot {} tuple end {} exceeds footer free offset {}",
                i, tuple_end, free_offset
            )));
        }

        for (other_slot, other_start, other_end) in &live_ranges {
            if tuple_start < *other_end && *other_start < tuple_end {
                return Err(heap_error(format!(
                    "heap page v1 tuple overlap: slot {} {}..{} overlaps slot {} {}..{}",
                    i, tuple_start, tuple_end, other_slot, other_start, other_end
                )));
            }
        }

        live_ranges.push((i, tuple_start, tuple_end));
        slots.push(entry);
    }

    Ok(slots)
}
