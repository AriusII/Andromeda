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
//! |    (grows upward from byte 96)   |  Variable-length tuple data
//! |                                  |
//! ~------ free space ------~
//! |                                  |
//! |    Slot Directory                |  Fixed-size slot entries (growing downward)
//! |    (slot 0 at page_size - 57)    |
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

use crate::PageSize;

#[cfg(test)]
use crate::heap_row_encoder::{ColumnDef, RowSchema, ScalarType};
use crate::heap_row_encoder::{Datum, RowEncoder};

const HEAP_PAGE_V1_HEADER_SIZE: usize = 96;
const HEAP_PAGE_V1_TRAILER_SIZE: usize = 48;
const HEAP_PAGE_V1_SLOT_METADATA_SIZE: usize = 4;
const HEAP_PAGE_V1_HEADER_SLOT_COUNT_OFFSET: usize = 40;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HeapPageV1SlotMetadata {
    slot_count: usize,
    free_offset: u16,
    metadata_offset: usize,
    slot_base: usize,
}

fn heap_page_v1_metadata_offset(page_size: PageSize) -> usize {
    page_size.bytes_usize() - HEAP_PAGE_V1_TRAILER_SIZE - HEAP_PAGE_V1_SLOT_METADATA_SIZE
}

fn heap_page_v1_read_slot_metadata(
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

    // DEC-032 blocks the legacy/header-only reader path. Header offset 40 is not
    // authoritative under HeapPageV1; if populated it is only accepted as a redundant
    // copy that matches the footer metadata.
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
        let metadata = heap_page_v1_read_slot_metadata(page_size, &data)?;
        let mut slot_directory = Vec::with_capacity(metadata.slot_count);

        for i in 0..metadata.slot_count {
            let slot_offset = metadata.metadata_offset - ((i + 1) * SlotEntry::SIZE);
            let mut slot_bytes = [0u8; 5];
            slot_bytes.copy_from_slice(&data[slot_offset..slot_offset + 5]);
            let entry = SlotEntry::from_bytes(slot_bytes);
            if !entry.is_deleted() {
                let tuple_start = entry.offset as usize;
                let tuple_end = tuple_start.saturating_add(entry.length as usize);
                if tuple_start < HEAP_PAGE_V1_HEADER_SIZE || tuple_end > metadata.slot_base {
                    return Err(heap_error(format!(
                        "heap page v1 slot {} tuple bounds {}..{} outside payload region {}..{}",
                        i, tuple_start, tuple_end, HEAP_PAGE_V1_HEADER_SIZE, metadata.slot_base
                    )));
                }
            }
            slot_directory.push(entry);
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

        // Find insertion offset (allocate tuple payloads contiguously after the page header).
        //
        // Slot entries are serialized from the end of the page, before the trailer. Keeping
        // tuple bytes growing upward from the header preserves a single free-space interval
        // between tuple payloads and the slot directory and prevents later slot entries from
        // overwriting earlier tuple bytes.
        let header_size = HEAP_PAGE_V1_HEADER_SIZE;
        let next_tuple_offset = self
            .slot_directory
            .iter()
            .filter(|entry| !entry.is_deleted())
            .map(|entry| entry.offset as usize + entry.length as usize)
            .max()
            .unwrap_or(header_size);
        let insert_offset = next_tuple_offset as u16;

        // Write tuple data
        let offset_usize = insert_offset as usize;
        self.data[offset_usize..offset_usize + tuple_len as usize].copy_from_slice(tuple);

        // Create and append slot entry
        let slot_id = self.slot_directory.len() as u16;
        self.slot_directory
            .push(SlotEntry::new(insert_offset, tuple_len));

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
                slot_id,
                offset,
                length,
                self.data.len()
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
    pub fn scan(&self) -> HeapScanIter<'_> {
        HeapScanIter {
            page: self,
            current_slot: 0,
        }
    }

    /// Delete multiple tuples by their slot IDs (batch logical deletion).
    ///
    /// Atomically marks all provided slot IDs as deleted. Does not reclaim space.
    /// Returns the number of slots successfully marked deleted.
    ///
    /// # Invariants
    /// - All slots in the batch must be valid (in range and not already deleted)
    /// - If any slot is invalid, returns error and no slots are marked
    /// - Partial batch deletion is not supported (all-or-nothing)
    pub fn mark_deleted_batch(&mut self, slot_ids: &[u16]) -> AndromedaResult<usize> {
        if slot_ids.is_empty() {
            return Ok(0);
        }

        // Validate all slot IDs before making any changes (all-or-nothing)
        for &slot_id in slot_ids {
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
        }

        // All valid: mark them all
        for &slot_id in slot_ids {
            let slot_id_usize = slot_id as usize;
            self.slot_directory[slot_id_usize].mark_deleted();
        }

        Ok(slot_ids.len())
    }

    /// Check if this page has any deleted slots.
    ///
    /// Returns true if at least one slot is marked deleted.
    /// Useful for deciding whether to compact.
    pub fn has_deleted_slots(&self) -> bool {
        self.slot_directory.iter().any(|e| e.is_deleted())
    }

    /// Compact the page by reclaiming space from deleted tuples.
    ///
    /// Moves all live tuples downward (toward lower offsets), eliminating fragmentation
    /// caused by deleted slots. Slot directory is rebuilt in order. Preserves all live
    /// tuple data and slot ordering.
    ///
    /// # Returns
    /// - Number of bytes reclaimed from deleted tuples
    ///
    /// # Invariants
    /// - All live tuples are preserved (no data loss)
    /// - Live tuple slot IDs remain unchanged
    /// - Slot directory is rebuilt but ordering preserved
    /// - Free space is increased by the sum of deleted tuple sizes
    ///
    /// # Performance
    /// - O(N) where N = number of live tuples
    /// - Approximately 5-10 microseconds for typical page
    pub fn compact_deleted(&mut self) -> AndromedaResult<u16> {
        // Calculate bytes to be reclaimed
        let reclaimed: u16 = self
            .slot_directory
            .iter()
            .filter(|e| e.is_deleted())
            .map(|e| e.length)
            .sum();

        if reclaimed == 0 {
            return Ok(0); // No deleted slots, nothing to do
        }

        // Build new data layout with only live tuples
        let header_size = HEAP_PAGE_V1_HEADER_SIZE;
        let trailer_size = HEAP_PAGE_V1_TRAILER_SIZE;
        let mut new_data = vec![0u8; self.data.len()];

        // Copy header
        new_data[..header_size].copy_from_slice(&self.data[..header_size]);

        // Copy trailer
        let trailer_start = self.data.len() - trailer_size;
        new_data[trailer_start..].copy_from_slice(&self.data[trailer_start..]);

        let mut new_offset = header_size as u16;
        let mut new_slot_directory = Vec::with_capacity(self.slot_directory.len());

        // Rewrite live tuple payloads while preserving slot IDs. Deleted slots stay in
        // the directory so external RowIds remain stable and continue to fail reads.
        for slot_entry in self.slot_directory.iter() {
            if !slot_entry.is_deleted() {
                let old_offset = slot_entry.offset as usize;
                let length = slot_entry.length as usize;

                new_data[new_offset as usize..new_offset as usize + length]
                    .copy_from_slice(&self.data[old_offset..old_offset + length]);

                new_slot_directory.push(SlotEntry::new(new_offset, slot_entry.length));
                new_offset = new_offset.saturating_add(slot_entry.length);
            } else {
                new_slot_directory.push(*slot_entry);
            }
        }

        // Update page state
        self.data = new_data;
        self.slot_directory = new_slot_directory;

        Ok(reclaimed)
    }

    /// Get free space in bytes.
    fn compute_free_space_bytes(&self) -> usize {
        let header_size = HEAP_PAGE_V1_HEADER_SIZE;
        let trailer_size = HEAP_PAGE_V1_TRAILER_SIZE;

        let next_tuple_offset = self
            .slot_directory
            .iter()
            .filter(|e| !e.is_deleted())
            .map(|e| e.offset as usize + e.length as usize)
            .max()
            .unwrap_or(header_size);
        let slot_directory_start =
            self.data.len() - trailer_size - (self.slot_directory.len() * SlotEntry::SIZE);

        slot_directory_start.saturating_sub(next_tuple_offset)
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

// ============================================================================
// Wave 21 Batch 3 Task 1: N1-HEAP-003 — Slot Directory
// ============================================================================
//
// Slot Directory: Manages tuple locations and lengths within a heap page
// using a growing-upward slot array paired with growing-downward data region.

pub mod slot_directory {
    use super::*;

    /// Unique identifier for a tuple slot within a page.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct SlotId(u16);

    impl SlotId {
        /// Create a new slot ID from raw value.
        pub fn new(value: u16) -> Self {
            Self(value)
        }

        /// Get the underlying slot ID value.
        pub fn get(self) -> u16 {
            self.0
        }

        /// Get as usize for array indexing.
        pub fn as_usize(self) -> usize {
            self.0 as usize
        }
    }

    /// Slot directory metadata for managing tuple locations within a page.
    ///
    /// Stores an array of slots where each slot tracks a tuple's offset and length.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SlotDirectory {
        page_size: PageSize,
        slots: Vec<SlotEntry>,
        /// Byte offset in page where free space begins (growing downward from data)
        free_offset: u16,
    }

    impl SlotDirectory {
        /// Create a new empty slot directory for the given page size.
        pub fn new(page_size: PageSize) -> Self {
            let header_size = HEAP_PAGE_V1_HEADER_SIZE as u16;
            Self {
                page_size,
                slots: Vec::new(),
                free_offset: header_size,
            }
        }

        /// Load slot directory from serialized page data.
        pub fn from_page_data(page_size: PageSize, page_data: &[u8]) -> AndromedaResult<Self> {
            let metadata = heap_page_v1_read_slot_metadata(page_size, page_data)?;

            let mut slots = Vec::with_capacity(metadata.slot_count);
            for i in 0..metadata.slot_count {
                let slot_offset = metadata.metadata_offset - ((i + 1) * SlotEntry::SIZE);
                let mut slot_bytes = [0u8; 5];
                slot_bytes.copy_from_slice(&page_data[slot_offset..slot_offset + 5]);
                let slot = SlotEntry::from_bytes(slot_bytes);
                if !slot.is_deleted() {
                    let tuple_start = slot.offset as usize;
                    let tuple_end = tuple_start.saturating_add(slot.length as usize);
                    if tuple_start < HEAP_PAGE_V1_HEADER_SIZE || tuple_end > metadata.slot_base {
                        return Err(heap_error(format!(
                            "heap page v1 slot {} tuple bounds {}..{} outside payload region {}..{}",
                            i, tuple_start, tuple_end, HEAP_PAGE_V1_HEADER_SIZE, metadata.slot_base
                        )));
                    }
                }
                slots.push(slot);
            }

            Ok(Self {
                page_size,
                slots,
                free_offset: metadata.free_offset,
            })
        }

        /// Allocate a new slot for a tuple of given length.
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

            // Try to reuse a deleted slot
            for (i, slot) in self.slots.iter_mut().enumerate() {
                if slot.is_deleted() && slot.offset == 0 {
                    *slot = SlotEntry::new(self.free_offset, length);
                    self.free_offset = self.free_offset.saturating_add(length);
                    return Ok(SlotId::new(i as u16));
                }
            }

            let max_slots = match self.page_size {
                PageSize::KiB16 => 256,
                PageSize::KiB32 => 512,
            };

            if self.slots.len() >= max_slots {
                return Err(heap_error(format!("page slot limit {} reached", max_slots)));
            }

            let slot_id = self.slots.len() as u16;
            self.slots.push(SlotEntry::new(self.free_offset, length));
            self.free_offset = self.free_offset.saturating_add(length);

            Ok(SlotId::new(slot_id))
        }

        /// Get the offset and length of a live slot.
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

        /// Mark a slot as logically deleted.
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

        /// Compact the slot directory by removing gaps from deleted tuples.
        pub fn compact(&mut self) -> u16 {
            if self.slots.last().is_some_and(|slot| slot.is_deleted()) {
                let freed_bytes = self.slots.pop().map(|slot| slot.length).unwrap_or(0);
                self.free_offset = self.free_offset.saturating_sub(freed_bytes);
                return freed_bytes;
            }
            0
        }

        /// Calculate available free space in the page.
        pub fn free_space(&self) -> u16 {
            let trailer_size = HEAP_PAGE_V1_TRAILER_SIZE as u16;
            let slot_directory_size = (self.slots.len() as u16) * SlotEntry::SIZE as u16
                + HEAP_PAGE_V1_SLOT_METADATA_SIZE as u16;
            let page_end = self.page_size.bytes() as u16;

            let allocated_end = self.free_offset;
            let directory_start = page_end.saturating_sub(trailer_size + slot_directory_size);

            directory_start.saturating_sub(allocated_end)
        }

        /// Get the total number of slots (including deleted).
        pub fn slot_count(&self) -> usize {
            self.slots.len()
        }

        /// Get the number of active (non-deleted) slots.
        pub fn active_slot_count(&self) -> usize {
            self.slots
                .iter()
                .filter(|s| !s.is_deleted() && s.offset > 0)
                .count()
        }

        /// Serialize slot directory to page layout format.
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

        /// Validate slot directory invariants.
        pub fn validate(&self) -> AndromedaResult<()> {
            let max_slots = match self.page_size {
                PageSize::KiB16 => 256,
                PageSize::KiB32 => 512,
            };
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
        use super::*;

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
}

// ============================================================================
// Wave 21 Batch 4 Task 3: N1-HEAP-005 — Heap Page Insert Operations
// ============================================================================
//
// HeapPageInsert: Implements tuple insertion on heap pages with full
// row encoding integration and buffer pool coordination.

/// Heap page insert operation context.
///
/// Manages tuple insertion with row encoding, slot directory allocation,
/// and buffer pool coordination.
///
/// ## Invariants
///
/// 1. **Page Capacity**: Tuple bytes + slot entry fit within page capacity
/// 2. **RowId Determinism**: Same datums at same page produce identical RowId
/// 3. **No Tuple Loss**: All inserted tuples remain until explicitly deleted
/// 4. **Slot Directory Integrity**: Slots never overlap; offsets monotonic
/// 5. **Encoding Determinism**: Row codec produces identical bytes for identical datums
///
/// ## Performance
///
/// - Typical insertion: 5-10 microseconds (small tuples)
/// - Encoding overhead: ~2-3 microseconds
/// - Slot allocation: ~1-2 microseconds
#[derive(Debug)]
pub struct HeapPageInsert {
    page_id: crate::PageId,
    page_size: PageSize,
    data: Vec<u8>,
    slot_directory: slot_directory::SlotDirectory,
    row_encoder: Option<RowEncoder>,
}

impl HeapPageInsert {
    /// Create a new heap page insert context for the given page.
    ///
    /// # Arguments
    ///
    /// * `page_id` - Durable page identifier
    /// * `page_size` - Page capacity (16 KiB or 32 KiB)
    ///
    /// # Errors
    ///
    /// - Invalid page size
    /// - Page size < 4 KiB
    pub fn new(page_id: crate::PageId, page_size: PageSize) -> AndromedaResult<Self> {
        if page_size.bytes() < 4096 {
            return Err(heap_error("page size must be >= 4 KiB"));
        }

        let size = page_size.bytes_usize();
        Ok(Self {
            page_id,
            page_size,
            data: vec![0u8; size],
            slot_directory: slot_directory::SlotDirectory::new(page_size),
            row_encoder: None,
        })
    }

    /// Set row encoder for this page (required before inserting structured rows).
    pub fn with_encoder(mut self, encoder: RowEncoder) -> Self {
        self.row_encoder = Some(encoder);
        self
    }

    /// Insert a raw tuple (byte array) into the page.
    ///
    /// Returns the slot ID assigned to the tuple.
    ///
    /// # Errors
    ///
    /// - Page full (insufficient space for tuple + slot entry)
    /// - Tuple too large (> u16::MAX)
    pub fn insert_raw_tuple(&mut self, tuple_bytes: &[u8]) -> AndromedaResult<u16> {
        if tuple_bytes.is_empty() {
            return Err(heap_error("tuple must not be empty"));
        }

        if tuple_bytes.len() > u16::MAX as usize {
            return Err(heap_error(format!(
                "tuple too large: {} bytes (max {})",
                tuple_bytes.len(),
                u16::MAX
            )));
        }

        let tuple_len = tuple_bytes.len() as u16;

        // Check free space (need space for tuple + new slot entry)
        let free_space = self.slot_directory.free_space();
        let needed = tuple_len as usize + SlotEntry::SIZE;

        if free_space < needed as u16 {
            return Err(heap_error(format!(
                "page full: need {} bytes, have {} bytes",
                needed, free_space as usize
            )));
        }

        // Allocate slot in directory
        let slot_id = self.slot_directory.allocate_slot(tuple_len)?;

        // Get allocated offset from slot directory
        let (offset, _) = self
            .slot_directory
            .get_slot(slot_id)
            .expect("just allocated")
            .ok_or_else(|| heap_error("slot allocation inconsistency"))?;

        // Write tuple data at offset
        let offset_usize = offset as usize;
        if offset_usize + tuple_bytes.len() > self.data.len() {
            return Err(heap_error(
                "page layout violation: tuple exceeds page bounds",
            ));
        }

        self.data[offset_usize..offset_usize + tuple_bytes.len()].copy_from_slice(tuple_bytes);

        Ok(slot_id.get())
    }

    /// Insert a structured row (vector of Datums) into the page.
    ///
    /// Encodes the row using the configured row encoder, then inserts
    /// the encoded bytes. Returns the slot ID.
    ///
    /// # Errors
    ///
    /// - No row encoder configured
    /// - Row encoding fails (invalid schema match, too large, etc.)
    /// - Page full
    pub fn insert_tuple(&mut self, datums: &[Datum]) -> AndromedaResult<u16> {
        let encoder = self
            .row_encoder
            .as_ref()
            .ok_or_else(|| heap_error("row encoder not configured"))?;

        let encoded = encoder.encode(datums)?;
        self.insert_raw_tuple(&encoded)
    }

    /// Read a tuple by slot ID.
    ///
    /// Returns the raw byte data.
    ///
    /// # Errors
    ///
    /// - Slot ID out of range
    /// - Slot marked deleted
    /// - Page corrupted
    pub fn read_tuple(&self, slot_id: u16) -> AndromedaResult<Vec<u8>> {
        let slot = slot_directory::SlotId::new(slot_id);
        let (offset, length) = self
            .slot_directory
            .get_slot(slot)?
            .ok_or_else(|| heap_error("slot deleted or not found"))?;

        let offset_usize = offset as usize;
        let length_usize = length as usize;

        if offset_usize + length_usize > self.data.len() {
            return Err(heap_error("page corrupted: slot offset out of bounds"));
        }

        Ok(self.data[offset_usize..offset_usize + length_usize].to_vec())
    }

    /// Delete a tuple by slot ID (logical deletion).
    pub fn delete_tuple(&mut self, slot_id: u16) -> AndromedaResult<()> {
        let slot = slot_directory::SlotId::new(slot_id);
        self.slot_directory.mark_deleted(slot)?;
        Ok(())
    }

    /// Get the page ID.
    pub fn page_id(&self) -> crate::PageId {
        self.page_id
    }

    /// Get the page size.
    pub fn page_size(&self) -> PageSize {
        self.page_size
    }

    /// Get current slot count.
    pub fn slot_count(&self) -> usize {
        self.slot_directory.slot_count()
    }

    /// Get active (non-deleted) slot count.
    pub fn active_slot_count(&self) -> usize {
        self.slot_directory.active_slot_count()
    }

    /// Get available free space in bytes.
    pub fn free_space(&self) -> u16 {
        self.slot_directory.free_space()
    }

    /// Serialize the page to byte array (for persistence).
    pub fn serialize(&mut self) -> AndromedaResult<Vec<u8>> {
        self.slot_directory.serialize_to_page(&mut self.data)?;
        Ok(self.data.clone())
    }
}

#[cfg(test)]
mod heap_insert_tests {
    use super::*;
    use std::sync::Arc;

    fn create_test_schema() -> Arc<RowSchema> {
        Arc::new(
            RowSchema::new(vec![
                ColumnDef {
                    name: "id".to_string(),
                    ordinal: 0,
                    scalar_type: ScalarType::Int64,
                    nullable: false,
                },
                ColumnDef {
                    name: "name".to_string(),
                    ordinal: 1,
                    scalar_type: ScalarType::UInt32,
                    nullable: true,
                },
                ColumnDef {
                    name: "active".to_string(),
                    ordinal: 2,
                    scalar_type: ScalarType::Bool,
                    nullable: true,
                },
            ])
            .expect("schema creation"),
        )
    }

    #[test]
    fn test_insert_raw_single_tuple() {
        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context");

        let data = b"hello_world";
        let slot_id = insert.insert_raw_tuple(data).expect("insert");

        assert_eq!(slot_id, 0);
        let read = insert.read_tuple(slot_id).expect("read");
        assert_eq!(read, data);
    }

    #[test]
    fn test_insert_multiple_tuples() {
        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context");

        let mut slot_ids = Vec::new();
        for i in 0..10 {
            let data = format!("tuple_{}", i);
            let slot_id = insert.insert_raw_tuple(data.as_bytes()).expect("insert");
            slot_ids.push(slot_id);
        }

        assert_eq!(slot_ids.len(), 10);
        for (i, slot_id) in slot_ids.iter().enumerate() {
            let data = format!("tuple_{}", i);
            let read = insert.read_tuple(*slot_id).expect("read");
            assert_eq!(read, data.as_bytes());
        }
    }

    #[test]
    fn test_insert_fill_page_boundary() {
        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context");

        let large_tuple = vec![42u8; 2000];
        let mut count = 0;

        loop {
            match insert.insert_raw_tuple(&large_tuple) {
                Ok(_) => count += 1,
                Err(e) if e.message().contains("full") => break,
                Err(e) => panic!("unexpected error: {}", e.message()),
            }
        }

        assert!(count > 0);
        // Final insert should fail
        let result = insert.insert_raw_tuple(&large_tuple);
        assert!(
            result.is_err() && result.unwrap_err().message().contains("full"),
            "page should be full"
        );
    }

    #[test]
    fn test_insert_zero_length_rejected() {
        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context");

        let result = insert.insert_raw_tuple(&[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("empty"));
    }

    #[test]
    fn test_insert_oversized_tuple_rejected() {
        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context");

        let oversized = vec![0u8; (u16::MAX as usize) + 1];
        let result = insert.insert_raw_tuple(&oversized);
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("too large"));
    }

    #[test]
    fn test_insert_structured_row_with_encoder() {
        let schema = create_test_schema();
        let encoder = RowEncoder::new(schema);

        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context")
            .with_encoder(encoder);

        let datums = vec![Datum::Int64(42), Datum::Null, Datum::Bool(true)];

        let slot_id = insert.insert_tuple(&datums).expect("insert");
        assert_eq!(slot_id, 0);
    }

    #[test]
    fn test_insert_without_encoder_fails() {
        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context");

        let datums = vec![Datum::Int64(42), Datum::Null, Datum::Bool(true)];

        let result = insert.insert_tuple(&datums);
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("encoder"));
    }

    #[test]
    fn test_delete_tuple_logical() {
        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context");

        let slot_id = insert.insert_raw_tuple(b"test").expect("insert");
        assert_eq!(insert.active_slot_count(), 1);

        insert.delete_tuple(slot_id).expect("delete");
        assert_eq!(insert.active_slot_count(), 0);

        // Deleted tuple cannot be read
        let result = insert.read_tuple(slot_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_page_metadata() {
        let insert = HeapPageInsert::new(crate::PageId::new(42), PageSize::KiB16)
            .expect("create insert context");

        assert_eq!(insert.page_id(), crate::PageId::new(42));
        assert_eq!(insert.page_size(), PageSize::KiB16);
        assert_eq!(insert.slot_count(), 0);
        assert_eq!(insert.active_slot_count(), 0);
        assert!(insert.free_space() > 0);
    }

    #[test]
    fn test_multiple_small_insertions() {
        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context");

        let mut ids = Vec::new();
        for i in 0..100 {
            let data = format!("s_{}", i);
            let id = insert.insert_raw_tuple(data.as_bytes()).expect("insert");
            ids.push(id);
        }

        assert_eq!(insert.active_slot_count(), 100);

        // Verify random samples
        for (i, &id) in ids.iter().enumerate() {
            let expected = format!("s_{}", i);
            let read = insert.read_tuple(id).expect("read");
            assert_eq!(read, expected.as_bytes());
        }
    }

    #[test]
    fn test_mixed_insert_delete_operations() {
        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context");

        let id1 = insert.insert_raw_tuple(b"tuple1").expect("insert 1");
        let id2 = insert.insert_raw_tuple(b"tuple2").expect("insert 2");
        let id3 = insert.insert_raw_tuple(b"tuple3").expect("insert 3");

        assert_eq!(insert.active_slot_count(), 3);

        insert.delete_tuple(id2).expect("delete");
        assert_eq!(insert.active_slot_count(), 2);

        // Can still read 1 and 3
        assert_eq!(insert.read_tuple(id1).expect("read 1"), b"tuple1");
        assert_eq!(insert.read_tuple(id3).expect("read 3"), b"tuple3");

        // Cannot read 2
        assert!(insert.read_tuple(id2).is_err());
    }

    #[test]
    fn test_encoding_roundtrip() {
        let schema = create_test_schema();
        let encoder = RowEncoder::new(schema.clone());

        let datums = vec![Datum::Int64(12345), Datum::Null, Datum::Bool(false)];

        // Encode, insert, read, decode
        let encoded = encoder.encode(&datums).expect("encode");
        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context");

        let slot_id = insert.insert_raw_tuple(&encoded).expect("insert");
        let decoded_bytes = insert.read_tuple(slot_id).expect("read");

        assert_eq!(encoded, decoded_bytes);
    }

    #[test]
    fn test_rowid_determinism() {
        // Same datums at same page should produce same RowId (slot assignment)
        let schema = create_test_schema();

        let mut insert1 = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create 1")
            .with_encoder(RowEncoder::new(schema.clone()));

        let mut insert2 = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create 2")
            .with_encoder(RowEncoder::new(schema.clone()));

        let datums = vec![Datum::Int64(999), Datum::Null, Datum::Bool(true)];

        let slot1 = insert1.insert_tuple(&datums).expect("insert 1");
        let slot2 = insert2.insert_tuple(&datums).expect("insert 2");

        // Same page, same datums → same slot ID
        assert_eq!(slot1, slot2);
    }

    #[test]
    fn test_serialize_page() {
        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context");

        insert.insert_raw_tuple(b"data1").expect("insert 1");
        insert.insert_raw_tuple(b"data2").expect("insert 2");

        let serialized = insert.serialize().expect("serialize");
        assert_eq!(serialized.len(), PageSize::KiB16.bytes_usize());
    }

    #[test]
    fn test_free_space_tracking() {
        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context");

        let initial = insert.free_space();
        let chunk = vec![0u8; 1000];

        insert.insert_raw_tuple(&chunk).expect("insert 1");
        let after1 = insert.free_space();

        assert!(after1 < initial);
        assert!(initial - after1 >= 1000);

        insert.insert_raw_tuple(&chunk).expect("insert 2");
        let after2 = insert.free_space();

        assert!(after2 < after1);
        assert!(after1 - after2 >= 1000);
    }
}
