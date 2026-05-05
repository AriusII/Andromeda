//! Heap Page Engine — Tuple storage, retrieval, and deletion operations.
//!
//! This module implements a variadic-length tuple heap storage engine for Andromeda.
//! All tuples are stored in a single heap page with dynamic slot directory management.
//!
//! ## Architecture
//!
//! ### Page Layout
//!
//! A heap page (16 KiB or 32 KiB) is divided into three regions:
//!
//! ```text
//! +----------------------------------+
//! |    PageHeader (96 bytes)         |  Fixed header with metadata
//! +----------------------------------+
//! |    Payload Data Region           |
//! |    (grows downward)              |  Variable-length tuple data
//! |                                  |
//! ~------ free space ------~
//! |                                  |
//! |    Slot Directory                |  Fixed-size slot entries (growing upward)
//! |    (grows upward)                |
//! +----------------------------------+
//! |    PageTrailer (48 bytes)        |  CRC and validation
//! +----------------------------------+
//! ```
//!
//! ### Slot Directory Entry (5 bytes)
//!
//! Each slot holds:
//! - `offset: u16` — Byte offset of tuple in page (0 = deleted)
//! - `length: u16` — Tuple length in bytes
//! - `flags: u8` — Bit 0: deleted flag, Bit 1: forwarded flag (future)
//!
//! ### Invariants
//!
//! 1. **Page Header Valid**: Validated via `PageLayoutContract`
//! 2. **Slot Ordering**: Slot IDs are 0-indexed, sequential
//! 3. **Tuple Non-Overlap**: No two tuple data regions overlap
//! 4. **Free Space Contiguous**: Unused bytes form single contiguous region
//! 5. **Deleted Mark**: Offset = 0 indicates logical deletion
//! 6. **No Unsafe Code**: Full forbid(unsafe_code)
//! 7. **Hot/Cold Agnostic**: Storage layout independent of HotStore/ColdStore tiers

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{PageSize};

pub use crate::heap_row_encoder::{RowEncoder, RowSchema, ScalarType, Datum, ColumnDef};

/// A single slot directory entry (5 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotEntry {
    /// Byte offset of tuple data within page (0 indicates deleted slot)
    offset: u16,
    /// Length of tuple data in bytes
    length: u16,
    /// Flags: bit 0 = deleted, bit 1 = forwarded (reserved)
    flags: u8,
}

impl SlotEntry {
    pub const SIZE: usize = 5;

    /// Create a new slot entry for a tuple at given offset with length.
    pub fn new(offset: u16, length: u16) -> Self {
        Self {
            offset,
            length,
            flags: 0,
        }
    }

    /// Mark this slot as deleted (logical deletion).
    pub fn mark_deleted(&mut self) {
        self.flags |= 0x01;
        self.offset = 0; // Clear offset on deletion
    }

    /// Check if this slot is marked as deleted.
    pub fn is_deleted(self) -> bool {
        (self.flags & 0x01) != 0
    }

    /// Get tuple offset if not deleted, else None.
    pub fn offset_if_live(self) -> Option<u16> {
        if self.is_deleted() {
            None
        } else {
            Some(self.offset)
        }
    }

    /// Serialize slot entry to 5-byte little-endian format.
    pub fn to_bytes(self) -> [u8; 5] {
        [
            (self.offset & 0xFF) as u8,
            ((self.offset >> 8) & 0xFF) as u8,
            (self.length & 0xFF) as u8,
            ((self.length >> 8) & 0xFF) as u8,
            self.flags,
        ]
    }

    /// Deserialize slot entry from 5-byte little-endian format.
    pub fn from_bytes(bytes: [u8; 5]) -> Self {
        let offset = u16::from_le_bytes([bytes[0], bytes[1]]);
        let length = u16::from_le_bytes([bytes[2], bytes[3]]);
        let flags = bytes[4];
        Self {
            offset,
            length,
            flags,
        }
    }
}

/// A variadic-length heap page for tuple storage.
///
/// Manages slot-based tuple storage with deletion via logical marking.
/// Single page only — cross-page transactions deferred.
#[derive(Debug, Clone)]
pub struct HeapPage {
    page_size: PageSize,
    data: Vec<u8>,
    slot_directory: Vec<SlotEntry>,
}

impl HeapPage {
    /// Create a new heap page with given page ID and size.
    pub fn new(page_size: PageSize) -> Self {
        let size = page_size.bytes_usize();
        let data = vec![0u8; size];

        Self {
            page_size,
            data,
            slot_directory: Vec::new(),
        }
    }

    /// Load heap page from raw page image bytes.
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

        // Extract slot directory from end of page
        // Slot directory follows PageTrailer (48 bytes before end)
        // We reconstruct it from PageHeader.slot_count
        let trailer_len = 48;
        let max_offset = size - trailer_len;

        // Read slot count from header (at offset 40)
        let slot_count = if data.len() >= 42 {
            u16::from_le_bytes([data[40], data[41]]) as usize
        } else {
            return Err(heap_error("page too short to read slot count"));
        };

        let mut slot_directory = Vec::with_capacity(slot_count);

        for i in 0..slot_count {
            let slot_offset = max_offset - ((i + 1) * SlotEntry::SIZE);
            if slot_offset < 96 {
                // 96 bytes for header
                return Err(heap_error("slot directory overlaps page header"));
            }

            let mut slot_bytes = [0u8; 5];
            slot_bytes.copy_from_slice(&data[slot_offset..slot_offset + 5]);
            slot_directory.push(SlotEntry::from_bytes(slot_bytes));
        }

        Ok(Self {
            page_size,
            data,
            slot_directory,
        })
    }

    /// Get page size.
    pub fn page_size(&self) -> PageSize {
        self.page_size
    }

    /// Get total number of slots (includes deleted ones).
    pub fn slot_count(&self) -> usize {
        self.slot_directory.len()
    }

    /// Count live (non-deleted) tuples.
    pub fn live_row_count(&self) -> u32 {
        self.slot_directory
            .iter()
            .filter(|e| !e.is_deleted())
            .count() as u32
    }

    /// Insert a tuple into the page.
    ///
    /// Returns the assigned slot ID if successful.
    /// Returns error if page has insufficient free space.
    pub fn insert_tuple(&mut self, tuple: &[u8]) -> AndromedaResult<u16> {
        if tuple.len() > u16::MAX as usize {
            return Err(heap_error("tuple too large: exceeds u16::MAX"));
        }

        let tuple_len = tuple.len() as u16;

        // Check free space (need space for tuple + new slot entry)
        let free_space = self.compute_free_space_bytes();
        let needed = tuple_len as usize + SlotEntry::SIZE;

        if free_space < needed {
            return Err(heap_error(format!(
                "heap page full: need {} bytes, have {} bytes",
                needed, free_space
            )));
        }

        // Find insertion offset (allocate from end of data region, before slot directory)
        let trailer_len = 48;
        let slot_dir_start = self.data.len() - trailer_len - (self.slot_directory.len() * SlotEntry::SIZE);
        let insert_offset = (slot_dir_start - tuple_len as usize) as u16;

        // Write tuple data
        let offset_usize = insert_offset as usize;
        self.data[offset_usize..offset_usize + tuple_len as usize].copy_from_slice(tuple);

        // Create and append slot entry
        let slot_id = self.slot_directory.len() as u16;
        self.slot_directory.push(SlotEntry::new(insert_offset, tuple_len));

        Ok(slot_id)
    }

    /// Read a tuple by slot ID.
    ///
    /// Returns error if slot ID invalid, slot deleted, or page corrupted.
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
                slot_id, offset, length, self.data.len()
            )));
        }

        Ok(self.data[offset..offset + length].to_vec())
    }

    /// Delete a tuple by marking its slot as deleted (logical deletion).
    ///
    /// Does not free physical space; space is reclaimed on page compaction (future work).
    pub fn delete_tuple(&mut self, slot_id: u16) -> AndromedaResult<()> {
        let slot_id_usize = slot_id as usize;

        if slot_id_usize >= self.slot_directory.len() {
            return Err(heap_error(format!(
                "slot {} out of range: page has {} slots",
                slot_id,
                self.slot_directory.len()
            )));
        }

        if self.slot_directory[slot_id_usize].is_deleted() {
            return Err(heap_error(format!("slot {} already deleted", slot_id)));
        }

        self.slot_directory[slot_id_usize].mark_deleted();
        Ok(())
    }

    /// Update a tuple: logical delete + new insert.
    ///
    /// Simple approach; in-place updates (if space permits) deferred to Wave 19.
    pub fn update_tuple(&mut self, slot_id: u16, new_tuple: &[u8]) -> AndromedaResult<u16> {
        self.delete_tuple(slot_id)?;
        self.insert_tuple(new_tuple)
    }

    /// Scan live tuples, yielding (slot_id, tuple_data) pairs.
    pub fn scan(&self) -> HeapScanIter {
        HeapScanIter {
            page: self,
            current_slot: 0,
        }
    }

    /// Get free space in bytes.
    fn compute_free_space_bytes(&self) -> usize {
        let header_size = 96;
        let trailer_size = 48;

        // Calculate used space by live tuples
        let used_by_tuples: usize = self
            .slot_directory
            .iter()
            .filter(|e| !e.is_deleted())
            .map(|e| e.length as usize)
            .sum();

        let used_by_slots = self.slot_directory.len() * SlotEntry::SIZE;

        self.data.len() - header_size - trailer_size - used_by_tuples - used_by_slots
    }

    /// Serialize slot directory to bytes (for persisting to page image).
    pub fn serialize_slot_directory(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.slot_directory.len() * SlotEntry::SIZE);
        for entry in self.slot_directory.iter().rev() {
            bytes.extend_from_slice(&entry.to_bytes());
        }
        bytes
    }
}

/// Iterator over live tuples in a heap page.
pub struct HeapScanIter<'a> {
    page: &'a HeapPage,
    current_slot: u16,
}

impl<'a> Iterator for HeapScanIter<'a> {
    type Item = AndromedaResult<(u16, Vec<u8>)>;

    fn next(&mut self) -> Option<Self::Item> {
        while (self.current_slot as usize) < self.page.slot_directory.len() {
            let slot_id = self.current_slot;
            self.current_slot += 1;

            match self.page.read_tuple(slot_id) {
                Ok(data) => return Some(Ok((slot_id, data))),
                Err(e) if e.message().contains("deleted") => continue,
                Err(e) => return Some(Err(e)),
            }
        }
        None
    }
}

/// Helper to create storage errors.
fn heap_error(msg: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, msg)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(result.err().unwrap().message().contains("deleted"));
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

        // Delete slot 1 and 3
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

        let slot_id = page
            .insert_tuple(b"original_data")
            .expect("insert failed");
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

        // Fill page with large tuples
        let large_tuple = vec![0u8; 2000]; // 2 KB tuples

        let mut inserted = 0;
        loop {
            match page.insert_tuple(&large_tuple) {
                Ok(_) => inserted += 1,
                Err(_) => break,
            }
        }

        assert!(inserted > 0);
        assert!(page.insert_tuple(&large_tuple).is_err());
    }

    #[test]
    fn test_slot_entry_serialization() {
        let entry = SlotEntry::new(100, 50);
        let bytes = entry.to_bytes();
        let deserialized = SlotEntry::from_bytes(bytes);

        assert_eq!(entry, deserialized);
    }

    #[test]
    fn test_slot_entry_deletion() {
        let mut entry = SlotEntry::new(100, 50);
        assert!(!entry.is_deleted());

        entry.mark_deleted();
        assert!(entry.is_deleted());
        assert_eq!(entry.offset, 0);
    }
}
