use andromeda_error::AndromedaResult;
use andromeda_storage_page::PageSize;

use super::{
    HEAP_PAGE_V1_PAYLOAD_OFFSET, HEAP_PAGE_V1_TRAILER_SIZE, SlotEntry, heap_error,
    heap_page_v1_read_and_validate_slots, heap_page_v1_read_slot_metadata,
    heap_page_v1_validate_format_guard, tuple_layout::read_tuple_bytes,
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
        debug_assert!(heap_page_v1_validate_format_guard().is_ok());
        let size = page_size.bytes_usize();
        let data = vec![0u8; size];

        Self {
            page_size,
            data,
            slot_directory: Vec::new(),
        }
    }

    pub fn from_image(page_size: PageSize, bytes: &[u8]) -> AndromedaResult<Self> {
        validate_page_image_size(page_size, bytes)?;
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(heap_error(
                "blank/unallocated heap page image requires explicit unallocated parser",
            ));
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

    pub fn from_blank_unallocated_image(
        page_size: PageSize,
        bytes: &[u8],
    ) -> AndromedaResult<Self> {
        validate_page_image_size(page_size, bytes)?;
        if !bytes.iter().all(|byte| *byte == 0) {
            return Err(heap_error(
                "explicit unallocated heap parser only accepts blank page images",
            ));
        }
        Ok(Self::new(page_size))
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
        let entry = self.slot_directory[self.checked_slot_index(slot_id)?];
        if entry.is_deleted() {
            return Err(heap_error(format!("slot {} has been deleted", slot_id)));
        }

        read_tuple_bytes(
            &self.data,
            entry.offset(),
            entry.length(),
            format!("slot {} offset/length overflows", slot_id),
            format!(
                "slot {} has invalid offset/length: offset={}, len={}, page_size={}",
                slot_id,
                entry.offset(),
                entry.length(),
                self.data.len()
            ),
        )
    }

    pub fn serialize_slot_directory(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.slot_directory.len() * SlotEntry::SIZE);
        for entry in self.slot_directory.iter().rev() {
            bytes.extend_from_slice(&entry.to_bytes());
        }
        bytes
    }

    pub(crate) fn compute_free_space_bytes(&self) -> usize {
        let slot_directory_start = self.data.len()
            - HEAP_PAGE_V1_TRAILER_SIZE
            - (self.slot_directory.len() * SlotEntry::SIZE);

        slot_directory_start.saturating_sub(self.next_tuple_offset())
    }

    pub(crate) fn checked_slot_index(&self, slot_id: u16) -> AndromedaResult<usize> {
        let slot_id_usize = slot_id as usize;
        if slot_id_usize >= self.slot_directory.len() {
            return Err(heap_error(format!(
                "slot {} out of range: page has {} slots",
                slot_id,
                self.slot_directory.len()
            )));
        }
        Ok(slot_id_usize)
    }

    pub(crate) fn next_tuple_offset(&self) -> usize {
        self.slot_directory
            .iter()
            .filter(|entry| !entry.is_deleted())
            .map(|entry| entry.offset() as usize + entry.length() as usize)
            .max()
            .unwrap_or(HEAP_PAGE_V1_PAYLOAD_OFFSET)
    }
}

fn validate_page_image_size(page_size: PageSize, bytes: &[u8]) -> AndromedaResult<()> {
    let size = page_size.bytes_usize();
    if bytes.len() != size {
        return Err(heap_error(format!(
            "page size mismatch: expected {} bytes, got {}",
            size,
            bytes.len()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use andromeda_storage_page::PageSize;

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
