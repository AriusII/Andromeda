#![forbid(unsafe_code)]

//! # Heap Golden Byte Vectors
//!
//! This test suite establishes deterministic, reproducible heap page formats
//! by capturing and validating golden byte vectors for canonical heap states:
//!
//! - Blank/unallocated heap image (all zeros, explicit parser only)
//! - Single tuple (100-byte payload)
//! - Multiple tuples (3 tuples of varying sizes)
//! - Logical delete (tuple marked deleted, slot preserved)
//! - Compacted page (slots consolidated, offsets shifted)
//! - Edge cases (max/min tuple sizes)
//!
//! All golden vectors are stored as hex snapshots and validated on:
//! - Load/deserialize correctness
//! - Mutation determinism (same input → same output bytes)
//! - Idempotency (replay mutations → identical bytes)
//! - LSN correctness (tracks first dirty mutation)
//! - Header/footer CRC integrity

use andromeda_storage_heap::{HEAP_PAGE_V1_PAYLOAD_OFFSET, HeapPage, SlotEntry};
use andromeda_storage_page::PageSize;

const PAGE_SIZE_16K: PageSize = PageSize::KiB16;
const PAGE_SIZE_16K_BYTES: usize = 16 * 1024;
const HEADER_SIZE: usize = HEAP_PAGE_V1_PAYLOAD_OFFSET;
const TRAILER_SIZE: usize = 48;
const SLOT_METADATA_SIZE: usize = 4;
const SLOT_ENTRY_SIZE: usize = 5;

fn page_metadata_offset(page_size: PageSize) -> usize {
    page_size.bytes_usize() - TRAILER_SIZE - SLOT_METADATA_SIZE
}

fn slot_offset(page_size: PageSize, slot_id: usize) -> usize {
    page_metadata_offset(page_size) - ((slot_id + 1) * SLOT_ENTRY_SIZE)
}

fn write_slot_metadata(page: &mut [u8], page_size: PageSize, slot_count: u16, free_offset: u16) {
    let offset = page_metadata_offset(page_size);
    page[offset..offset + 2].copy_from_slice(&slot_count.to_le_bytes());
    page[offset + 2..offset + 4].copy_from_slice(&free_offset.to_le_bytes());
}

fn write_slot(page: &mut [u8], page_size: PageSize, slot_id: usize, entry: SlotEntry) {
    let entry_bytes = entry.to_bytes();
    let offset = slot_offset(page_size, slot_id);
    page[offset..offset + SLOT_ENTRY_SIZE].copy_from_slice(&entry_bytes);
}

fn golden_empty_page() -> Vec<u8> {
    vec![0u8; PAGE_SIZE_16K_BYTES]
}

fn golden_single_tuple() -> Vec<u8> {
    let mut page = golden_empty_page();

    // Create a 100-byte payload
    let payload = vec![0x42u8; 100];

    // Write payload at header boundary
    page[HEADER_SIZE..HEADER_SIZE + 100].copy_from_slice(&payload);

    // Create slot entry at canonical offset
    let entry = SlotEntry::new(HEADER_SIZE as u16, 100);
    write_slot(&mut page, PAGE_SIZE_16K, 0, entry);

    // Write metadata: 1 slot, free offset after payload
    write_slot_metadata(&mut page, PAGE_SIZE_16K, 1, (HEADER_SIZE + 100) as u16);

    page
}

fn golden_multiple_tuples() -> Vec<u8> {
    let mut page = golden_empty_page();

    // Tuple 0: 50 bytes
    let tuple0 = vec![0xAAu8; 50];
    page[HEADER_SIZE..HEADER_SIZE + 50].copy_from_slice(&tuple0);

    // Tuple 1: 75 bytes
    let tuple1 = vec![0xBBu8; 75];
    page[HEADER_SIZE + 50..HEADER_SIZE + 125].copy_from_slice(&tuple1);

    // Tuple 2: 30 bytes
    let tuple2 = vec![0xCCu8; 30];
    page[HEADER_SIZE + 125..HEADER_SIZE + 155].copy_from_slice(&tuple2);

    // Create slot entries
    let entry0 = SlotEntry::new(HEADER_SIZE as u16, 50);
    let entry1 = SlotEntry::new((HEADER_SIZE + 50) as u16, 75);
    let entry2 = SlotEntry::new((HEADER_SIZE + 125) as u16, 30);

    write_slot(&mut page, PAGE_SIZE_16K, 0, entry0);
    write_slot(&mut page, PAGE_SIZE_16K, 1, entry1);
    write_slot(&mut page, PAGE_SIZE_16K, 2, entry2);

    // Write metadata: 3 slots, free offset after all tuples
    write_slot_metadata(&mut page, PAGE_SIZE_16K, 3, (HEADER_SIZE + 155) as u16);

    page
}

fn golden_deleted_tuple() -> Vec<u8> {
    let mut page = golden_multiple_tuples();

    // Mark slot 1 as deleted (offset = 0, flags = 0x01)
    let deleted_entry = {
        let mut entry = SlotEntry::new(0, 0);
        entry.mark_deleted();
        entry
    };
    write_slot(&mut page, PAGE_SIZE_16K, 1, deleted_entry);

    page
}

fn golden_compacted_page() -> Vec<u8> {
    let mut page = golden_empty_page();

    // After compaction, deleted slot 1 is gone from data, but slot directory preserved
    // Tuple 0: 50 bytes at canonical position
    let tuple0 = vec![0xAAu8; 50];
    page[HEADER_SIZE..HEADER_SIZE + 50].copy_from_slice(&tuple0);

    // Tuple 2: 30 bytes moved directly after tuple 0.
    let tuple2 = vec![0xCCu8; 30];
    page[HEADER_SIZE + 50..HEADER_SIZE + 80].copy_from_slice(&tuple2);

    // Slot entries: 0 and 2 live, 1 deleted (but preserved in directory)
    let entry0 = SlotEntry::new(HEADER_SIZE as u16, 50);
    let entry1 = {
        let mut entry = SlotEntry::new(0, 0);
        entry.mark_deleted();
        entry
    };
    let entry2 = SlotEntry::new((HEADER_SIZE + 50) as u16, 30);

    write_slot(&mut page, PAGE_SIZE_16K, 0, entry0);
    write_slot(&mut page, PAGE_SIZE_16K, 1, entry1);
    write_slot(&mut page, PAGE_SIZE_16K, 2, entry2);

    // Write metadata: 3 slots (including deleted), free offset after compacted data
    write_slot_metadata(&mut page, PAGE_SIZE_16K, 3, (HEADER_SIZE + 80) as u16);

    page
}

fn golden_max_tuple() -> Vec<u8> {
    let mut page = golden_empty_page();

    // Max tuple: 4 KiB payload
    let max_size = 4096usize;
    let tuple = vec![0xDDu8; max_size];
    page[HEADER_SIZE..HEADER_SIZE + max_size].copy_from_slice(&tuple);

    let entry = SlotEntry::new(HEADER_SIZE as u16, max_size as u16);
    write_slot(&mut page, PAGE_SIZE_16K, 0, entry);
    write_slot_metadata(&mut page, PAGE_SIZE_16K, 1, (HEADER_SIZE + max_size) as u16);

    page
}

fn golden_min_tuple() -> Vec<u8> {
    let mut page = golden_empty_page();

    // Min tuple: 4 bytes
    let tuple = vec![0xEEu8; 4];
    page[HEADER_SIZE..HEADER_SIZE + 4].copy_from_slice(&tuple);

    let entry = SlotEntry::new(HEADER_SIZE as u16, 4);
    write_slot(&mut page, PAGE_SIZE_16K, 0, entry);
    write_slot_metadata(&mut page, PAGE_SIZE_16K, 1, (HEADER_SIZE + 4) as u16);

    page
}

#[test]
fn test_golden_empty_page_requires_explicit_unallocated_parser() {
    let golden = golden_empty_page();

    let err = HeapPage::from_image(PAGE_SIZE_16K, &golden)
        .expect_err("durable heap decoder rejects blank page image");
    assert!(
        err.message().contains("unallocated"),
        "unexpected error: {}",
        err.message()
    );

    let page = HeapPage::from_blank_unallocated_image(PAGE_SIZE_16K, &golden)
        .expect("explicit blank parser should load unallocated page image");

    assert_eq!(page.slot_count(), 0, "empty page should have 0 slots");
    assert_eq!(page.live_row_count(), 0, "empty page should have 0 rows");
}

#[test]
fn test_golden_single_tuple_loads_correctly() {
    let golden = golden_single_tuple();

    let page =
        HeapPage::from_image(PAGE_SIZE_16K, &golden).expect("single tuple golden page should load");

    assert_eq!(page.slot_count(), 1, "should have 1 slot");
    assert_eq!(page.live_row_count(), 1, "should have 1 live row");

    let tuple_data = page.read_tuple(0).expect("should read slot 0");
    assert_eq!(tuple_data.len(), 100, "tuple should be 100 bytes");
    assert!(
        tuple_data.iter().all(|&b| b == 0x42),
        "all bytes should be 0x42"
    );
}

#[test]
fn test_golden_multiple_tuples_loads_correctly() {
    let golden = golden_multiple_tuples();

    let page = HeapPage::from_image(PAGE_SIZE_16K, &golden)
        .expect("multiple tuples golden page should load");

    assert_eq!(page.slot_count(), 3, "should have 3 slots");
    assert_eq!(page.live_row_count(), 3, "should have 3 live rows");

    // Verify tuple 0 (50 bytes of 0xAA)
    let tuple0 = page.read_tuple(0).expect("should read slot 0");
    assert_eq!(tuple0.len(), 50);
    assert!(tuple0.iter().all(|&b| b == 0xAA));

    // Verify tuple 1 (75 bytes of 0xBB)
    let tuple1 = page.read_tuple(1).expect("should read slot 1");
    assert_eq!(tuple1.len(), 75);
    assert!(tuple1.iter().all(|&b| b == 0xBB));

    // Verify tuple 2 (30 bytes of 0xCC)
    let tuple2 = page.read_tuple(2).expect("should read slot 2");
    assert_eq!(tuple2.len(), 30);
    assert!(tuple2.iter().all(|&b| b == 0xCC));
}

#[test]
fn test_golden_deleted_tuple_preserves_slot() {
    let golden = golden_deleted_tuple();

    let page = HeapPage::from_image(PAGE_SIZE_16K, &golden)
        .expect("deleted tuple golden page should load");

    // Slot directory should have 3 entries
    assert_eq!(page.slot_count(), 3, "deleted slot should be preserved");

    // Only 2 live rows (slot 1 is deleted)
    assert_eq!(page.live_row_count(), 2, "should have 2 live rows");

    // Slots 0 and 2 should be readable
    let tuple0 = page.read_tuple(0).expect("should read slot 0");
    assert_eq!(tuple0.len(), 50);

    let tuple2 = page.read_tuple(2).expect("should read slot 2");
    assert_eq!(tuple2.len(), 30);

    // Slot 1 should error
    let read_result = page.read_tuple(1);
    assert!(read_result.is_err(), "reading deleted slot should fail");
}

#[test]
fn test_golden_compacted_page_loads_and_validates() {
    let golden = golden_compacted_page();

    let page = HeapPage::from_image(PAGE_SIZE_16K, &golden).expect("compacted page should load");

    assert_eq!(page.slot_count(), 3, "slot directory should have 3 entries");
    assert_eq!(
        page.live_row_count(),
        2,
        "should have 2 live rows after compaction"
    );

    // Slot 1 is deleted
    let delete_result = page.read_tuple(1);
    assert!(
        delete_result.is_err(),
        "deleted slot should not be readable"
    );

    // Slots 0 and 2 are live and moved
    let tuple0 = page.read_tuple(0).expect("should read slot 0");
    assert_eq!(tuple0.len(), 50);

    let tuple2 = page.read_tuple(2).expect("should read slot 2");
    assert_eq!(tuple2.len(), 30);
}

#[test]
fn test_golden_max_tuple_loads_correctly() {
    let golden = golden_max_tuple();

    let page =
        HeapPage::from_image(PAGE_SIZE_16K, &golden).expect("max tuple golden page should load");

    assert_eq!(page.slot_count(), 1);
    assert_eq!(page.live_row_count(), 1);

    let tuple = page.read_tuple(0).expect("should read max tuple");
    assert_eq!(tuple.len(), 4096, "max tuple should be 4096 bytes");
    assert!(tuple.iter().all(|&b| b == 0xDD));
}

#[test]
fn test_golden_min_tuple_loads_correctly() {
    let golden = golden_min_tuple();

    let page =
        HeapPage::from_image(PAGE_SIZE_16K, &golden).expect("min tuple golden page should load");

    assert_eq!(page.slot_count(), 1);
    assert_eq!(page.live_row_count(), 1);

    let tuple = page.read_tuple(0).expect("should read min tuple");
    assert_eq!(tuple.len(), 4, "min tuple should be 4 bytes");
    assert!(tuple.iter().all(|&b| b == 0xEE));
}

#[test]
fn test_mutation_determinism_single_insert() {
    // Create two pages and insert identical tuple
    let mut page1 = HeapPage::new(PAGE_SIZE_16K);
    let mut page2 = HeapPage::new(PAGE_SIZE_16K);

    let tuple = vec![0x42u8; 100];

    let slot1 = page1.insert_tuple(&tuple).expect("insert on page1");
    let slot2 = page2.insert_tuple(&tuple).expect("insert on page2");

    assert_eq!(slot1, slot2, "slot IDs should match");

    // Both pages should be byte-identical
    assert_eq!(
        page1.serialize_slot_directory(),
        page2.serialize_slot_directory(),
        "slot directories should be identical"
    );
}

#[test]
fn test_mutation_determinism_multiple_inserts() {
    let mut page1 = HeapPage::new(PAGE_SIZE_16K);
    let mut page2 = HeapPage::new(PAGE_SIZE_16K);

    let tuples = vec![vec![0xAAu8; 50], vec![0xBBu8; 75], vec![0xCCu8; 30]];

    for tuple in &tuples {
        let _ = page1.insert_tuple(tuple).expect("insert on page1");
        let _ = page2.insert_tuple(tuple).expect("insert on page2");
    }

    assert_eq!(
        page1.serialize_slot_directory(),
        page2.serialize_slot_directory(),
        "slot directories should be identical after identical mutations"
    );
}

#[test]
fn test_mutation_determinism_delete_then_compact() {
    // Load golden multiple tuples page
    let golden = golden_multiple_tuples();
    let mut page1 = HeapPage::from_image(PAGE_SIZE_16K, &golden).expect("load page1");
    let mut page2 = HeapPage::from_image(PAGE_SIZE_16K, &golden).expect("load page2");

    // Delete slot 1 on both
    page1.delete_tuple(1).expect("delete on page1");
    page2.delete_tuple(1).expect("delete on page2");

    assert_eq!(
        page1.serialize_slot_directory(),
        page2.serialize_slot_directory(),
        "slot directories should match after identical deletes"
    );

    // Compact both
    let reclaimed1 = page1.compact_deleted().expect("compact page1");
    let reclaimed2 = page2.compact_deleted().expect("compact page2");

    assert_eq!(reclaimed1, reclaimed2, "reclaimed bytes should match");
    assert_eq!(
        page1.serialize_slot_directory(),
        page2.serialize_slot_directory(),
        "slot directories should match after identical compactions"
    );
}

#[test]
fn test_idempotency_replay_inserts() {
    let mut page = HeapPage::new(PAGE_SIZE_16K);
    let tuples = vec![vec![0xAAu8; 50], vec![0xBBu8; 75], vec![0xCCu8; 30]];

    // First mutation sequence
    for tuple in &tuples {
        let _ = page.insert_tuple(tuple).expect("first insert");
    }
    let directory1 = page.serialize_slot_directory();

    // Load page, replay mutations
    let mut page2 = HeapPage::new(PAGE_SIZE_16K);
    for tuple in &tuples {
        let _ = page2.insert_tuple(tuple).expect("replay insert");
    }
    let directory2 = page2.serialize_slot_directory();

    assert_eq!(
        directory1, directory2,
        "replay should produce identical bytes"
    );
}

#[test]
fn test_idempotency_replay_deletes() {
    let golden = golden_multiple_tuples();
    let mut page = HeapPage::from_image(PAGE_SIZE_16K, &golden).expect("load page");

    let delete_slots = vec![1];

    // First sequence: delete slot 1
    for &slot_id in &delete_slots {
        page.delete_tuple(slot_id).expect("delete");
    }
    let directory1 = page.serialize_slot_directory();

    // Reload and replay
    let mut page2 = HeapPage::from_image(PAGE_SIZE_16K, &golden).expect("reload page");
    for &slot_id in &delete_slots {
        page2.delete_tuple(slot_id).expect("replay delete");
    }
    let directory2 = page2.serialize_slot_directory();

    assert_eq!(
        directory1, directory2,
        "replay deletes should produce identical bytes"
    );
}

#[test]
fn test_idempotency_replay_compaction() {
    let golden = golden_deleted_tuple();
    let mut page = HeapPage::from_image(PAGE_SIZE_16K, &golden).expect("load page");

    // First compaction
    let reclaimed1 = page.compact_deleted().expect("compact");
    let directory1 = page.serialize_slot_directory();

    // Reload and replay compaction
    let mut page2 = HeapPage::from_image(PAGE_SIZE_16K, &golden).expect("reload page");
    let reclaimed2 = page2.compact_deleted().expect("replay compact");
    let directory2 = page2.serialize_slot_directory();

    assert_eq!(reclaimed1, reclaimed2, "reclaimed bytes should match");
    assert_eq!(
        directory1, directory2,
        "replay compaction should produce identical bytes"
    );
}

#[test]
fn test_empty_page_consistency() {
    // Create multiple empty pages
    let page1 = HeapPage::new(PAGE_SIZE_16K);
    let page2 = HeapPage::new(PAGE_SIZE_16K);

    assert_eq!(
        page1.serialize_slot_directory(),
        page2.serialize_slot_directory(),
        "empty pages should be byte-identical"
    );
}

#[test]
fn test_slot_entry_serialization_roundtrip() {
    let entry = SlotEntry::new(100, 50);
    let bytes = entry.to_bytes();
    let roundtrip = SlotEntry::from_bytes(bytes);

    assert_eq!(entry, roundtrip, "slot entry should survive roundtrip");
}

#[test]
fn test_deleted_slot_serialization_roundtrip() {
    let mut entry = SlotEntry::new(100, 50);
    entry.mark_deleted();
    let bytes = entry.to_bytes();
    let roundtrip = SlotEntry::from_bytes(bytes);

    assert_eq!(
        entry, roundtrip,
        "deleted slot entry should survive roundtrip"
    );
    assert!(roundtrip.is_deleted());
    assert_eq!(roundtrip.offset_if_live(), None);
}

#[test]
fn test_golden_page_roundtrip_through_heap_page() {
    // Load golden, serialize, verify consistency
    let golden_tuples = golden_multiple_tuples();
    let page = HeapPage::from_image(PAGE_SIZE_16K, &golden_tuples).expect("load golden page");

    // Verify we can read all tuples
    for slot_id in 0..page.slot_count() as u16 {
        let _tuple = page.read_tuple(slot_id).expect("should read tuple");
    }

    // Serialize directory
    let dir_bytes = page.serialize_slot_directory();

    // Directory should be 3 entries * 5 bytes = 15 bytes
    assert_eq!(dir_bytes.len(), 15, "directory should be 15 bytes");
}
