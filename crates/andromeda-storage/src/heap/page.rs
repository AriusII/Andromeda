use andromeda_core::AndromedaResult;

use crate::PageSize;

use super::{
    HEAP_PAGE_V1_HEADER_SIZE, HEAP_PAGE_V1_TRAILER_SIZE, SlotEntry, heap_error,
    heap_page_v1_read_and_validate_slots, heap_page_v1_read_slot_metadata,
};

/// A variadic-length heap page for tuple storage.
#[derive(Debug, Clone)]
pub struct HeapPage {
    pub(crate) page_size: PageSize,
    pub(crate) data: Vec<u8>,
    pub(crate) slot_directory: Vec<SlotEntry>,
}

impl HeapPage {
    pub fn new(page_size: PageSize) -> Self {
        let size = page_size.bytes_usize();
        let data = vec![0u8; size];

        Self {
            page_size,
            data,
            slot_directory: Vec::new(),
        }
    }

    pub fn from_image(page_size: PageSize, bytes: &[u8]) -> AndromedaResult<Self> {
        let size = page_size.bytes_usize();
        if bytes.len() != size {
            return Err(heap_error(format!(
                "page size mismatch: expected {} bytes, got {}",
                size,
                bytes.len()
            )));
        }

        let data = bytes.to_vec();
        let metadata = heap_page_v1_read_slot_metadata(page_size, &data)?;
        let slot_directory = heap_page_v1_read_and_validate_slots(&data, metadata)?;

        Ok(Self {
            page_size,
            data,
            slot_directory,
        })
    }

    pub fn page_size(&self) -> PageSize {
        self.page_size
    }

    pub fn slot_count(&self) -> usize {
        self.slot_directory.len()
    }

    pub fn live_row_count(&self) -> u32 {
        self.slot_directory
            .iter()
            .filter(|e| !e.is_deleted())
            .count() as u32
    }

    pub fn read_tuple(&self, slot_id: u16) -> AndromedaResult<Vec<u8>> {
        let slot_id_usize = slot_id as usize;
        if slot_id_usize >= self.slot_directory.len() {
            return Err(heap_error(format!(
                "slot {} out of range: page has {} slots",
                slot_id,
                self.slot_directory.len()
            )));
        }

        let entry = self.slot_directory[slot_id_usize];
        if entry.is_deleted() {
            return Err(heap_error(format!("slot {} has been deleted", slot_id)));
        }

        let offset = entry.offset as usize;
        let length = entry.length as usize;
        if offset + length > self.data.len() {
            return Err(heap_error(format!(
                "slot {} has invalid offset/length: offset={}, len={}, page_size={}",
                slot_id,
                offset,
                length,
                self.data.len()
            )));
        }

        Ok(self.data[offset..offset + length].to_vec())
    }

    pub fn serialize_slot_directory(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.slot_directory.len() * SlotEntry::SIZE);
        for entry in self.slot_directory.iter().rev() {
            bytes.extend_from_slice(&entry.to_bytes());
        }
        bytes
    }

    pub(crate) fn compute_free_space_bytes(&self) -> usize {
        let next_tuple_offset = self
            .slot_directory
            .iter()
            .filter(|e| !e.is_deleted())
            .map(|e| e.offset as usize + e.length as usize)
            .max()
            .unwrap_or(HEAP_PAGE_V1_HEADER_SIZE);
        let slot_directory_start = self.data.len()
            - HEAP_PAGE_V1_TRAILER_SIZE
            - (self.slot_directory.len() * SlotEntry::SIZE);

        slot_directory_start.saturating_sub(next_tuple_offset)
    }
}

#[cfg(test)]
mod tests {
    use crate::PageSize;

    use super::HeapPage;

    #[test]
    fn test_heap_insert_read() {
        let mut page = HeapPage::new(PageSize::KiB16);
        let tuple = b"test_data";

        let slot_id = page.insert_tuple(tuple).expect("insert failed");
        let read_data = page.read_tuple(slot_id).expect("read failed");

        assert_eq!(read_data, tuple);
    }

    #[test]
    fn test_heap_delete() {
        let mut page = HeapPage::new(PageSize::KiB16);
        let tuple = b"test_data";

        let slot_id = page.insert_tuple(tuple).expect("insert failed");
        page.delete_tuple(slot_id).expect("delete failed");

        let result = page.read_tuple(slot_id);
        assert!(result.is_err());
        assert!(result.expect_err("error").message().contains("deleted"));
    }

    #[test]
    fn test_heap_multiple_inserts() {
        let mut page = HeapPage::new(PageSize::KiB16);

        let slot_ids: Vec<u16> = (0..10)
            .map(|i| {
                let tuple = format!("tuple_{}", i);
                page.insert_tuple(tuple.as_bytes()).expect("insert failed")
            })
            .collect();

        for (i, slot_id) in slot_ids.iter().enumerate() {
            let expected = format!("tuple_{}", i);
            let data = page.read_tuple(*slot_id).expect("read failed");
            assert_eq!(data, expected.as_bytes());
        }
    }

    #[test]
    fn test_heap_scan() {
        let mut page = HeapPage::new(PageSize::KiB16);

        for i in 0..5 {
            let tuple = format!("tuple_{}", i);
            page.insert_tuple(tuple.as_bytes()).expect("insert failed");
        }

        let scanned: Vec<_> = page
            .scan()
            .collect::<Result<Vec<_>, _>>()
            .expect("scan failed");

        assert_eq!(scanned.len(), 5);
    }

    #[test]
    fn test_heap_scan_with_deletions() {
        let mut page = HeapPage::new(PageSize::KiB16);

        let ids: Vec<u16> = (0..5)
            .map(|i| {
                let tuple = format!("tuple_{}", i);
                page.insert_tuple(tuple.as_bytes()).expect("insert failed")
            })
            .collect();

        page.delete_tuple(ids[1]).expect("delete failed");
        page.delete_tuple(ids[3]).expect("delete failed");

        let scanned: Vec<_> = page
            .scan()
            .collect::<Result<Vec<_>, _>>()
            .expect("scan failed");

        assert_eq!(scanned.len(), 3);
    }

    #[test]
    fn test_heap_update() {
        let mut page = HeapPage::new(PageSize::KiB16);

        let slot_id = page.insert_tuple(b"original_data").expect("insert failed");
        let new_slot = page
            .update_tuple(slot_id, b"updated_data")
            .expect("update failed");

        let result = page.read_tuple(slot_id);
        assert!(result.is_err());

        let data = page.read_tuple(new_slot).expect("read new failed");
        assert_eq!(data, b"updated_data");
    }

    #[test]
    fn test_heap_page_full() {
        let mut page = HeapPage::new(PageSize::KiB16);
        let large_tuple = vec![0u8; 2000];

        let mut inserted = 0;
        while page.insert_tuple(&large_tuple).is_ok() {
            inserted += 1;
        }

        assert!(inserted > 0);
        assert!(page.insert_tuple(&large_tuple).is_err());
    }
}
