#![forbid(unsafe_code)]

use andromeda_storage::PageSize;
use andromeda_storage::slot_directory::{SlotDirectory, SlotId};

// ============================================================================
// Wave 21 Batch 3 Task 1: N1-HEAP-003 — Heap Slot Directory Contract Tests
// ============================================================================
//
// This test suite validates the heap slot directory implementation with
// comprehensive test coverage including:
// - Basic allocation and retrieval
// - Deletion and compaction
// - Free space management
// - Boundary conditions
// - Page size variants
// - Fragmentation patterns

/// Test 1: Create empty slot directory
#[test]
fn test_slot_directory_create_empty() {
    let dir = SlotDirectory::new(PageSize::KiB16);
    assert_eq!(dir.slot_count(), 0);
    assert_eq!(dir.active_slot_count(), 0);
    assert!(dir.free_space() > 0);
}

/// Test 2: Allocate single slot
#[test]
fn test_slot_directory_allocate_single() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let slot_id = dir.allocate_slot(100).expect("allocation should succeed");

    assert_eq!(slot_id.get(), 0);
    assert_eq!(dir.slot_count(), 1);
    assert_eq!(dir.active_slot_count(), 1);

    let (offset, length) = dir
        .get_slot(slot_id)
        .expect("get_slot")
        .expect("slot exists");
    assert_eq!(offset, 96); // After page header
    assert_eq!(length, 100);
}

/// Test 3: Allocate multiple slots in sequence
#[test]
fn test_slot_directory_allocate_sequence() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let slot1 = dir.allocate_slot(100).expect("alloc 1");
    let slot2 = dir.allocate_slot(200).expect("alloc 2");
    let slot3 = dir.allocate_slot(150).expect("alloc 3");

    assert_eq!(dir.slot_count(), 3);
    assert_eq!(dir.active_slot_count(), 3);

    let (o1, l1) = dir.get_slot(slot1).expect("get 1").expect("slot 1");
    assert_eq!((o1, l1), (96, 100));

    let (o2, l2) = dir.get_slot(slot2).expect("get 2").expect("slot 2");
    assert_eq!((o2, l2), (196, 200));

    let (o3, l3) = dir.get_slot(slot3).expect("get 3").expect("slot 3");
    assert_eq!((o3, l3), (396, 150));
}

/// Test 4: Get non-existent slot returns None
#[test]
fn test_slot_directory_get_nonexistent() {
    let dir = SlotDirectory::new(PageSize::KiB16);
    let result = dir.get_slot(SlotId::new(0)).expect("get succeeds");
    assert!(result.is_none());
}

/// Test 5: Mark slot deleted
#[test]
fn test_slot_directory_mark_deleted() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let slot = dir.allocate_slot(100).expect("alloc");

    assert_eq!(dir.active_slot_count(), 1);

    dir.mark_deleted(slot).expect("mark_deleted succeeds");

    assert_eq!(dir.slot_count(), 1); // Slot still exists
    assert_eq!(dir.active_slot_count(), 0); // But not active
    assert!(dir.get_slot(slot).expect("get_slot").is_none()); // Returns None
}

/// Test 6: Cannot mark already-deleted slot
#[test]
fn test_slot_directory_double_delete_error() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let slot = dir.allocate_slot(100).expect("alloc");

    dir.mark_deleted(slot).expect("first delete");

    let result = dir.mark_deleted(slot);
    assert!(result.is_err(), "second delete should fail");
}

/// Test 7: Compact removes deleted tail slots
#[test]
fn test_slot_directory_compact_removes_deleted_tail() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let _slot1 = dir.allocate_slot(100).expect("alloc 1");
    let _slot2 = dir.allocate_slot(200).expect("alloc 2");
    let slot3 = dir.allocate_slot(150).expect("alloc 3");

    assert_eq!(dir.slot_count(), 3);

    dir.mark_deleted(slot3).expect("delete 3");
    assert_eq!(dir.slot_count(), 3);

    let freed = dir.compact();

    assert_eq!(freed, 150, "should free 150 bytes");
    assert_eq!(dir.slot_count(), 2, "should remove tail deleted slot");
    assert_eq!(dir.active_slot_count(), 2);

    // slot3 should no longer exist
    assert!(dir.get_slot(slot3).expect("get_slot").is_none());
}

/// Test 8: Compact with gaps in middle
#[test]
fn test_slot_directory_compact_with_gaps() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let _slot1 = dir.allocate_slot(100).expect("alloc 1");
    let slot2 = dir.allocate_slot(200).expect("alloc 2");
    let _slot3 = dir.allocate_slot(150).expect("alloc 3");
    let slot4 = dir.allocate_slot(75).expect("alloc 4");

    // Delete middle slot (slot2)
    dir.mark_deleted(slot2).expect("delete 2");
    assert_eq!(dir.active_slot_count(), 3);

    // Compact only removes deleted TAIL slots
    let freed = dir.compact();

    // Should free 0 because slot2 is not at the tail
    assert_eq!(freed, 0);
    assert_eq!(dir.slot_count(), 4, "all slots still present");

    // Now delete tail
    dir.mark_deleted(slot4).expect("delete 4");
    let freed2 = dir.compact();
    assert_eq!(freed2, 75);
    assert_eq!(dir.slot_count(), 3, "should remove tail slot 4");
}

/// Test 9: Free space calculation
#[test]
fn test_slot_directory_free_space() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let initial_free = dir.free_space();
    assert!(initial_free > 0, "should have initial free space");

    dir.allocate_slot(1000).expect("alloc 1000");
    let after_alloc = dir.free_space();

    assert!(after_alloc < initial_free, "free space should decrease");
    let used = initial_free.saturating_sub(after_alloc);
    assert_eq!(used, 1000, "should account for allocated bytes");
}

/// Test 10: Allocate zero-length slot fails
#[test]
fn test_slot_directory_allocate_zero_length_fails() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let result = dir.allocate_slot(0);
    assert!(result.is_err(), "zero-length allocation should fail");
}

/// Test 11: Allocate excessive length fails
#[test]
fn test_slot_directory_allocate_exceeds_capacity() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let excessive = 20000u16; // 20KB > 16KB page
    let result = dir.allocate_slot(excessive);
    assert!(result.is_err(), "excessive allocation should fail");
}

/// Test 12: Mark invalid slot ID fails
#[test]
fn test_slot_directory_mark_deleted_invalid_slot() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let result = dir.mark_deleted(SlotId::new(0));
    assert!(result.is_err(), "invalid slot ID should fail");
}

/// Test 13: Get slot by ID returns correct offset/length
#[test]
fn test_slot_directory_get_slot_accuracy() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let slot = dir.allocate_slot(512).expect("alloc");

    let result = dir.get_slot(slot).expect("get_slot").expect("should exist");
    assert_eq!(result.0, 96, "offset should be after header");
    assert_eq!(result.1, 512, "length should match");
}

/// Test 14: Slot ID type operations
#[test]
fn test_slot_id_operations() {
    let slot_id = SlotId::new(42);
    assert_eq!(slot_id.get(), 42);
    assert_eq!(slot_id.as_usize(), 42usize);

    let slot_id2 = SlotId::new(42);
    assert_eq!(slot_id, slot_id2);

    let slot_id3 = SlotId::new(41);
    assert!(slot_id > slot_id3);
}

/// Test 15: Validate empty directory
#[test]
fn test_slot_directory_validate_empty() {
    let dir = SlotDirectory::new(PageSize::KiB16);
    dir.validate().expect("empty directory should validate");
}

/// Test 16: Validate directory with slots
#[test]
fn test_slot_directory_validate_with_slots() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    dir.allocate_slot(100).expect("alloc");
    dir.allocate_slot(200).expect("alloc");
    dir.allocate_slot(150).expect("alloc");

    dir.validate().expect("should validate with no overlaps");
}

/// Test 17: Page size 32KB variant
#[test]
fn test_slot_directory_page_size_32kb() {
    let mut dir = SlotDirectory::new(PageSize::KiB32);

    // 32KB page should support more space
    for i in 0..5 {
        let size = 1000u16 + (i * 100u16);
        let result = dir.allocate_slot(size);
        assert!(result.is_ok(), "allocation {} should succeed", i);
    }

    assert_eq!(dir.slot_count(), 5);
    assert_eq!(dir.active_slot_count(), 5);
}

/// Test 18: Max slots for 16KB page
#[test]
fn test_slot_directory_16kb_max_slots() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    // Should be able to allocate ~256 slots max (small sizes to fit)
    let mut count = 0;
    for _ in 0..300 {
        match dir.allocate_slot(10) {
            Ok(_) => count += 1,
            Err(_) => break, // Ran out of space
        }
    }

    assert!(count > 0, "should allocate some slots");
    assert!(count <= 256, "should not exceed max slots");
}

/// Test 19: Serialize and deserialize
#[test]
fn test_slot_directory_serialize_deserialize() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let _slot1 = dir.allocate_slot(100).expect("alloc 1");
    let _slot2 = dir.allocate_slot(200).expect("alloc 2");

    // Create page buffer
    let mut page_data = vec![0u8; 16384]; // 16 KiB
    dir.serialize_to_page(&mut page_data).expect("serialize");

    // Deserialize and verify
    let restored = SlotDirectory::from_page_data(PageSize::KiB16, &page_data).expect("deserialize");

    assert_eq!(restored.slot_count(), dir.slot_count());
    assert_eq!(restored.active_slot_count(), dir.active_slot_count());
}

/// Test 20: Reuse deleted slot on reallocation
#[test]
fn test_slot_directory_reuse_deleted_slot() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let slot1 = dir.allocate_slot(100).expect("alloc 1");
    dir.allocate_slot(200).expect("alloc 2");

    assert_eq!(dir.slot_count(), 2);

    // Delete first slot
    dir.mark_deleted(slot1).expect("delete 1");
    assert_eq!(dir.active_slot_count(), 1);

    // Allocate new slot - should reuse slot1's ID
    let slot3 = dir.allocate_slot(150).expect("alloc 3");
    assert_eq!(slot3.get(), 0, "should reuse first slot ID");
    assert_eq!(dir.slot_count(), 2, "total slots unchanged");
    assert_eq!(dir.active_slot_count(), 2, "active slots restored");
}

/// Test 21: Fragmentation pattern stress test
#[test]
fn test_slot_directory_fragmentation_pattern() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    // Allocate pattern: large, small, large, small
    let large1 = dir.allocate_slot(500).expect("large 1");
    let small1 = dir.allocate_slot(50).expect("small 1");
    let large2 = dir.allocate_slot(500).expect("large 2");
    let small2 = dir.allocate_slot(50).expect("small 2");

    assert_eq!(dir.slot_count(), 4);

    // Delete alternating: delete small slots
    dir.mark_deleted(small1).expect("delete small 1");
    dir.mark_deleted(small2).expect("delete small 2");

    assert_eq!(dir.active_slot_count(), 2);

    // Compact should remove nothing (gaps in middle)
    let freed = dir.compact();
    assert_eq!(freed, 0, "should not remove middle gaps on compact");

    // Verify large slots still exist
    assert!(dir.get_slot(large1).expect("get").is_some());
    assert!(dir.get_slot(large2).expect("get").is_some());
}

/// Test 22: Cross-check slot offsets don't overlap
#[test]
fn test_slot_directory_no_tuple_overlap() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let slot1 = dir.allocate_slot(100).expect("alloc 1");
    let slot2 = dir.allocate_slot(150).expect("alloc 2");
    let slot3 = dir.allocate_slot(200).expect("alloc 3");

    let (o1, l1) = dir.get_slot(slot1).expect("get").expect("slot 1");
    let (o2, l2) = dir.get_slot(slot2).expect("get").expect("slot 2");
    let (o3, l3) = dir.get_slot(slot3).expect("get").expect("slot 3");

    // Verify no overlap
    let end1 = o1 as u32 + l1 as u32;
    let end2 = o2 as u32 + l2 as u32;

    assert!(end1 <= o2 as u32, "slot 1 should not overlap slot 2");
    assert!(end2 <= o3 as u32, "slot 2 should not overlap slot 3");

    // Validate should succeed
    dir.validate().expect("should validate");
}

/// Test 23: Active count accuracy
#[test]
fn test_slot_directory_active_count_accuracy() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    for _ in 0..10 {
        dir.allocate_slot(50).expect("alloc");
    }

    assert_eq!(dir.active_slot_count(), 10);

    // Delete 3 slots
    dir.mark_deleted(SlotId::new(0)).expect("delete");
    dir.mark_deleted(SlotId::new(2)).expect("delete");
    dir.mark_deleted(SlotId::new(5)).expect("delete");

    assert_eq!(dir.active_slot_count(), 7, "should have 7 active slots");

    // Compact won't remove these (not tail)
    dir.compact();
    assert_eq!(
        dir.active_slot_count(),
        7,
        "active count unchanged after compact"
    );
}

/// Test 24: Valid large tuple allocation
#[test]
fn test_slot_directory_large_tuple() {
    let mut dir = SlotDirectory::new(PageSize::KiB32); // Use 32KB page

    // Allocate 8KB tuple
    let slot = dir.allocate_slot(8192).expect("allocate 8KB");
    let (offset, length) = dir.get_slot(slot).expect("get").expect("exists");
    assert_eq!(length, 8192);
    assert!(offset > 0);
}

/// Test 25: Boundary between header and data region
#[test]
fn test_slot_directory_header_boundary() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    // First allocation should start right after 96-byte header
    let slot1 = dir.allocate_slot(100).expect("alloc");
    let (offset1, _length1) = dir.get_slot(slot1).expect("get").expect("exists");

    assert_eq!(
        offset1, 96,
        "first slot should start at offset 96 (after header)"
    );
}

/// Test 26: Concurrent allocation patterns (simulated)
#[test]
fn test_slot_directory_allocation_patterns() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    // Pattern: alternate between large and small
    let slots: Vec<_> = (0..10)
        .map(|i| {
            let size = if i % 2 == 0 { 200 } else { 50 };
            dir.allocate_slot(size).expect("alloc")
        })
        .collect();

    assert_eq!(dir.slot_count(), 10);
    assert_eq!(dir.active_slot_count(), 10);

    // Verify all are retrievable
    for slot_id in &slots {
        assert!(dir.get_slot(*slot_id).expect("get").is_some());
    }
}

/// Test 27: Compact with all deleted slots
#[test]
fn test_slot_directory_compact_all_deleted() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let slot1 = dir.allocate_slot(100).expect("alloc 1");
    let slot2 = dir.allocate_slot(200).expect("alloc 2");

    dir.mark_deleted(slot1).expect("delete 1");
    dir.mark_deleted(slot2).expect("delete 2");

    assert_eq!(dir.active_slot_count(), 0);

    let freed = dir.compact();
    assert_eq!(freed, 200, "should free slot 2");
    assert_eq!(dir.slot_count(), 1, "should have 1 slot remaining");

    let freed2 = dir.compact();
    assert_eq!(freed2, 100, "should free slot 1 in second compact");
    assert_eq!(dir.slot_count(), 0, "should be empty");
}

/// Test 28: Free space decreases with allocations
#[test]
fn test_slot_directory_free_space_monotonic() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let free1 = dir.free_space();
    dir.allocate_slot(100).expect("alloc 1");
    let free2 = dir.free_space();
    dir.allocate_slot(100).expect("alloc 2");
    let free3 = dir.free_space();
    dir.allocate_slot(100).expect("alloc 3");
    let free4 = dir.free_space();

    assert!(free1 > free2);
    assert!(free2 > free3);
    assert!(free3 > free4);
}

/// Test 29: Page size 16KB vs 32KB capacity
#[test]
fn test_slot_directory_page_size_capacity() {
    let mut dir16 = SlotDirectory::new(PageSize::KiB16);
    let mut dir32 = SlotDirectory::new(PageSize::KiB32);

    let test_size = 5000u16;

    let result16 = dir16.allocate_slot(test_size);
    let result32 = dir32.allocate_slot(test_size);

    // Both might succeed, but 32KB should have more remaining space
    let free16 = dir16.free_space();
    let free32 = dir32.free_space();

    // 32KB page should have significantly more free space than 16KB
    assert!(free32 > free16 || (result16.is_err() && result32.is_ok()));
}

/// Test 30: Serialize preserves structure
#[test]
fn test_slot_directory_serialize_preserves_structure() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let slots: Vec<_> = (0..5)
        .map(|i| dir.allocate_slot(100 + (i * 50u16)).expect("alloc"))
        .collect();

    // Delete some
    dir.mark_deleted(slots[1]).expect("delete");
    dir.mark_deleted(slots[3]).expect("delete");

    // Serialize
    let mut page_data = vec![0u8; 16384];
    dir.serialize_to_page(&mut page_data).expect("serialize");

    // Deserialize
    let restored = SlotDirectory::from_page_data(PageSize::KiB16, &page_data).expect("deserialize");

    // Verify structure matches
    assert_eq!(restored.slot_count(), dir.slot_count());
    assert_eq!(restored.active_slot_count(), dir.active_slot_count());

    // Verify specific slots
    assert!(restored.get_slot(slots[0]).expect("get").is_some());
    assert!(restored.get_slot(slots[1]).expect("get").is_none()); // deleted
    assert!(restored.get_slot(slots[2]).expect("get").is_some());
    assert!(restored.get_slot(slots[3]).expect("get").is_none()); // deleted
    assert!(restored.get_slot(slots[4]).expect("get").is_some());
}
