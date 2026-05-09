use andromeda_error::AndromedaResult;

use super::{HEAP_PAGE_V1_PAYLOAD_OFFSET, HEAP_PAGE_V1_TRAILER_SIZE, HeapPage, SlotEntry};

impl HeapPage {
    /// Compact deleted tuples while preserving slot IDs for live rows.
    pub fn compact_deleted(&mut self) -> AndromedaResult<u16> {
        let reclaimed: u16 = self
            .slot_directory
            .iter()
            .filter(|e| e.is_deleted())
            .map(|e| e.length())
            .sum();

        if reclaimed == 0 {
            return Ok(0);
        }

        let mut new_data = vec![0u8; self.data.len()];
        new_data[..HEAP_PAGE_V1_PAYLOAD_OFFSET]
            .copy_from_slice(&self.data[..HEAP_PAGE_V1_PAYLOAD_OFFSET]);

        let trailer_start = self.data.len() - HEAP_PAGE_V1_TRAILER_SIZE;
        new_data[trailer_start..].copy_from_slice(&self.data[trailer_start..]);

        let mut new_offset = HEAP_PAGE_V1_PAYLOAD_OFFSET as u16;
        let mut new_slot_directory = Vec::with_capacity(self.slot_directory.len());

        // Slot ids stay stable: deleted slots remain deleted and live slots are rewritten.
        for slot_entry in self.slot_directory.iter() {
            if !slot_entry.is_deleted() {
                let old_offset = slot_entry.offset() as usize;
                let length = slot_entry.length() as usize;

                new_data[new_offset as usize..new_offset as usize + length]
                    .copy_from_slice(&self.data[old_offset..old_offset + length]);

                new_slot_directory.push(SlotEntry::new(new_offset, slot_entry.length()));
                new_offset = new_offset.saturating_add(slot_entry.length());
            } else {
                new_slot_directory.push(*slot_entry);
            }
        }

        self.data = new_data;
        self.slot_directory = new_slot_directory;

        Ok(reclaimed)
    }
}
