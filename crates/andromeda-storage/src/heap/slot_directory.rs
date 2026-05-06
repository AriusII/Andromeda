use andromeda_core::AndromedaResult;

use crate::PageSize;

use super::{
    HEAP_PAGE_V1_HEADER_SIZE, HEAP_PAGE_V1_SLOT_METADATA_SIZE, HEAP_PAGE_V1_TRAILER_SIZE,
    SlotEntry, heap_error, heap_page_v1_max_slots, heap_page_v1_metadata_offset,
    heap_page_v1_read_and_validate_slots, heap_page_v1_read_slot_metadata,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SlotId(u16);

impl SlotId {
    pub fn new(value: u16) -> Self {
        Self(value)
    }

    pub fn get(self) -> u16 {
        self.0
    }

    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotDirectory {
    page_size: PageSize,
    slots: Vec<SlotEntry>,
    free_offset: u16,
}

impl SlotDirectory {
    pub fn new(page_size: PageSize) -> Self {
        let header_size = HEAP_PAGE_V1_HEADER_SIZE as u16;
        Self {
            page_size,
            slots: Vec::new(),
            free_offset: header_size,
        }
    }

    pub fn from_page_data(page_size: PageSize, page_data: &[u8]) -> AndromedaResult<Self> {
        let metadata = heap_page_v1_read_slot_metadata(page_size, page_data)?;
        let slots = heap_page_v1_read_and_validate_slots(page_data, metadata)?;

        Ok(Self {
            page_size,
            slots,
            free_offset: metadata.free_offset,
        })
    }

    pub fn allocate_slot(&mut self, length: u16) -> AndromedaResult<SlotId> {
        if length == 0 {
            return Err(heap_error("tuple length must be > 0"));
        }

        let can_reuse_deleted_slot = self
            .slots
            .iter()
            .any(|slot| slot.is_deleted() && slot.offset == 0);
        let slot_entry_space = if can_reuse_deleted_slot {
            0u16
        } else {
            SlotEntry::SIZE as u16
        };
        let required_space = length.saturating_add(slot_entry_space);
        if required_space > self.free_space() {
            return Err(heap_error("insufficient free space for tuple"));
        }

        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_deleted() && slot.offset == 0 {
                *slot = SlotEntry::new(self.free_offset, length);
                self.free_offset = self.free_offset.saturating_add(length);
                return Ok(SlotId::new(i as u16));
            }
        }

        let max_slots = heap_page_v1_max_slots(self.page_size);
        if self.slots.len() >= max_slots {
            return Err(heap_error(format!("page slot limit {} reached", max_slots)));
        }

        let slot_id = self.slots.len() as u16;
        self.slots.push(SlotEntry::new(self.free_offset, length));
        self.free_offset = self.free_offset.saturating_add(length);

        Ok(SlotId::new(slot_id))
    }

    pub fn get_slot(&self, slot_id: SlotId) -> AndromedaResult<Option<(u16, u16)>> {
        let idx = slot_id.as_usize();
        if idx >= self.slots.len() {
            return Ok(None);
        }

        let slot = self.slots[idx];
        if !slot.is_deleted() && slot.offset > 0 {
            Ok(Some((slot.offset, slot.length)))
        } else {
            Ok(None)
        }
    }

    pub fn mark_deleted(&mut self, slot_id: SlotId) -> AndromedaResult<()> {
        let idx = slot_id.as_usize();
        if idx >= self.slots.len() {
            return Err(heap_error("slot ID out of range"));
        }

        if self.slots[idx].is_deleted() {
            return Err(heap_error("slot already deleted"));
        }

        self.slots[idx].mark_deleted();
        Ok(())
    }

    pub fn compact(&mut self) -> u16 {
        if self.slots.last().is_some_and(|slot| slot.is_deleted()) {
            let freed_bytes = self.slots.pop().map(|slot| slot.length).unwrap_or(0);
            self.free_offset = self.free_offset.saturating_sub(freed_bytes);
            return freed_bytes;
        }
        0
    }

    pub fn free_space(&self) -> u16 {
        let trailer_size = HEAP_PAGE_V1_TRAILER_SIZE as u16;
        let slot_directory_size = (self.slots.len() as u16) * SlotEntry::SIZE as u16
            + HEAP_PAGE_V1_SLOT_METADATA_SIZE as u16;
        let page_end = self.page_size.bytes() as u16;

        let allocated_end = self.free_offset;
        let directory_start = page_end.saturating_sub(trailer_size + slot_directory_size);

        directory_start.saturating_sub(allocated_end)
    }

    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub fn active_slot_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|s| !s.is_deleted() && s.offset > 0)
            .count()
    }

    pub fn serialize_to_page(&self, page_data: &mut [u8]) -> AndromedaResult<()> {
        if page_data.len() != self.page_size.bytes_usize() {
            return Err(heap_error("page size mismatch"));
        }

        let metadata_offset = heap_page_v1_metadata_offset(self.page_size);

        for (i, slot) in self.slots.iter().enumerate() {
            let slot_offset = metadata_offset - ((i + 1) * SlotEntry::SIZE);
            if slot_offset < HEAP_PAGE_V1_HEADER_SIZE {
                return Err(heap_error("slot directory overlaps page header"));
            }
            let slot_bytes = slot.to_bytes();
            page_data[slot_offset..slot_offset + 5].copy_from_slice(&slot_bytes);
        }

        page_data[metadata_offset..metadata_offset + 2]
            .copy_from_slice(&(self.slots.len() as u16).to_le_bytes());
        page_data[metadata_offset + 2..metadata_offset + 4]
            .copy_from_slice(&self.free_offset.to_le_bytes());

        Ok(())
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        let max_slots = heap_page_v1_max_slots(self.page_size);
        if self.slots.len() > max_slots {
            return Err(heap_error(format!(
                "slot count {} exceeds limit {}",
                self.slots.len(),
                max_slots
            )));
        }

        for (i, slot_i) in self.slots.iter().enumerate() {
            if slot_i.is_deleted() {
                continue;
            }
            for (j, slot_j) in self.slots.iter().enumerate() {
                if i >= j || slot_j.is_deleted() {
                    continue;
                }

                let i_end = slot_i.offset.saturating_add(slot_i.length);
                let j_end = slot_j.offset.saturating_add(slot_j.length);

                if slot_i.offset < j_end && slot_j.offset < i_end {
                    return Err(heap_error(format!(
                        "tuple overlap: slot {} and slot {}",
                        i, j
                    )));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::PageSize;

    use super::{SlotDirectory, SlotEntry};

    #[test]
    fn test_slot_directory_new() {
        let dir = SlotDirectory::new(PageSize::KiB16);
        assert_eq!(dir.slot_count(), 0);
        assert_eq!(dir.active_slot_count(), 0);
    }

    #[test]
    fn test_allocate_single_slot() {
        let mut dir = SlotDirectory::new(PageSize::KiB16);
        let slot_id = dir.allocate_slot(100).expect("allocation");
        assert_eq!(slot_id.get(), 0);
        assert_eq!(dir.slot_count(), 1);

        let (offset, length) = dir.get_slot(slot_id).expect("get").expect("exists");
        assert_eq!((offset, length), (96, 100));
    }

    #[test]
    fn test_allocate_multiple_slots() {
        let mut dir = SlotDirectory::new(PageSize::KiB16);
        let slot1 = dir.allocate_slot(100).expect("alloc");
        let slot2 = dir.allocate_slot(200).expect("alloc");
        let slot3 = dir.allocate_slot(150).expect("alloc");

        assert_eq!(dir.slot_count(), 3);

        let (o1, l1) = dir.get_slot(slot1).expect("get").expect("exists");
        assert_eq!((o1, l1), (96, 100));
        let (o2, l2) = dir.get_slot(slot2).expect("get").expect("exists");
        assert_eq!((o2, l2), (196, 200));
        let (o3, l3) = dir.get_slot(slot3).expect("get").expect("exists");
        assert_eq!((o3, l3), (396, 150));
    }

    #[test]
    fn test_mark_deleted() {
        let mut dir = SlotDirectory::new(PageSize::KiB16);
        let slot = dir.allocate_slot(100).expect("alloc");
        assert_eq!(dir.active_slot_count(), 1);

        dir.mark_deleted(slot).expect("delete");
        assert_eq!(dir.active_slot_count(), 0);
        assert!(dir.get_slot(slot).expect("get").is_none());
    }

    #[test]
    fn test_compact() {
        let mut dir = SlotDirectory::new(PageSize::KiB16);
        let _slot1 = dir.allocate_slot(100).expect("alloc");
        let _slot2 = dir.allocate_slot(200).expect("alloc");
        let slot3 = dir.allocate_slot(150).expect("alloc");

        dir.mark_deleted(slot3).expect("delete");
        let freed = dir.compact();

        assert_eq!(freed, 150);
        assert_eq!(dir.slot_count(), 2);
    }

    #[test]
    fn test_free_space() {
        let mut dir = SlotDirectory::new(PageSize::KiB16);
        let initial = dir.free_space();
        assert!(initial > 0);

        dir.allocate_slot(1000).expect("alloc");
        let after = dir.free_space();

        assert!(after < initial);
        assert_eq!(initial - after, 1000 + SlotEntry::SIZE as u16);
    }

    #[test]
    fn test_validate() {
        let mut dir = SlotDirectory::new(PageSize::KiB16);
        dir.allocate_slot(100).expect("alloc");
        dir.allocate_slot(200).expect("alloc");

        dir.validate().expect("should validate");
    }
}
