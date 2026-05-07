use andromeda_storage::PageSize;
use andromeda_storage::slot_directory::{SlotDirectory, SlotId};

use crate::support::HEADER_SIZE;

#[test]
fn test_slot_directory_create_empty() {
    let dir = SlotDirectory::new(PageSize::KiB16);
    assert_eq!(dir.slot_count(), 0);
    assert_eq!(dir.active_slot_count(), 0);
    assert!(dir.free_space() > 0);
}

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
    assert_eq!(offset, HEADER_SIZE as u16);
    assert_eq!(length, 100);
}

#[test]
fn test_slot_directory_allocate_sequence() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let slot1 = dir.allocate_slot(100).expect("alloc 1");
    let slot2 = dir.allocate_slot(200).expect("alloc 2");
    let slot3 = dir.allocate_slot(150).expect("alloc 3");

    assert_eq!(dir.slot_count(), 3);
    assert_eq!(dir.active_slot_count(), 3);

    let (o1, l1) = dir.get_slot(slot1).expect("get 1").expect("slot 1");
    assert_eq!((o1, l1), (HEADER_SIZE as u16, 100));

    let (o2, l2) = dir.get_slot(slot2).expect("get 2").expect("slot 2");
    assert_eq!((o2, l2), ((HEADER_SIZE + 100) as u16, 200));

    let (o3, l3) = dir.get_slot(slot3).expect("get 3").expect("slot 3");
    assert_eq!((o3, l3), ((HEADER_SIZE + 300) as u16, 150));
}

#[test]
fn test_slot_directory_get_nonexistent() {
    let dir = SlotDirectory::new(PageSize::KiB16);
    let result = dir.get_slot(SlotId::new(0)).expect("get succeeds");
    assert!(result.is_none());
}

#[test]
fn test_slot_directory_mark_deleted() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let slot = dir.allocate_slot(100).expect("alloc");

    assert_eq!(dir.active_slot_count(), 1);

    dir.mark_deleted(slot).expect("mark_deleted succeeds");

    assert_eq!(dir.slot_count(), 1);
    assert_eq!(dir.active_slot_count(), 0);
    assert!(dir.get_slot(slot).expect("get_slot").is_none());
}

#[test]
fn test_slot_directory_double_delete_error() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let slot = dir.allocate_slot(100).expect("alloc");

    dir.mark_deleted(slot).expect("first delete");

    let result = dir.mark_deleted(slot);
    assert!(result.is_err(), "second delete should fail");
}

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
    assert!(dir.get_slot(slot3).expect("get_slot").is_none());
}

#[test]
fn test_slot_directory_compact_with_gaps() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let _slot1 = dir.allocate_slot(100).expect("alloc 1");
    let slot2 = dir.allocate_slot(200).expect("alloc 2");
    let _slot3 = dir.allocate_slot(150).expect("alloc 3");
    let slot4 = dir.allocate_slot(75).expect("alloc 4");

    dir.mark_deleted(slot2).expect("delete 2");
    assert_eq!(dir.active_slot_count(), 3);

    let freed = dir.compact();

    assert_eq!(freed, 0);
    assert_eq!(dir.slot_count(), 4, "all slots still present");

    dir.mark_deleted(slot4).expect("delete 4");
    let freed2 = dir.compact();
    assert_eq!(freed2, 75);
    assert_eq!(dir.slot_count(), 3, "should remove tail slot 4");
}

#[test]
fn test_slot_directory_free_space() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let initial_free = dir.free_space();
    assert!(initial_free > 0, "should have initial free space");

    dir.allocate_slot(1000).expect("alloc 1000");
    let after_alloc = dir.free_space();

    assert!(after_alloc < initial_free, "free space should decrease");
    let used = initial_free.saturating_sub(after_alloc);
    assert_eq!(
        used, 1005,
        "should account for tuple bytes and slot metadata"
    );
}

#[test]
fn test_slot_directory_allocate_zero_length_fails() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let result = dir.allocate_slot(0);
    assert!(result.is_err(), "zero-length allocation should fail");
}

#[test]
fn test_slot_directory_allocate_exceeds_capacity() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let excessive = 20000u16;
    let result = dir.allocate_slot(excessive);
    assert!(result.is_err(), "excessive allocation should fail");
}

#[test]
fn test_slot_directory_mark_deleted_invalid_slot() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let result = dir.mark_deleted(SlotId::new(0));
    assert!(result.is_err(), "invalid slot ID should fail");
}

#[test]
fn test_slot_directory_get_slot_accuracy() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let slot = dir.allocate_slot(512).expect("alloc");

    let result = dir.get_slot(slot).expect("get_slot").expect("should exist");
    assert_eq!(
        result.0, HEADER_SIZE as u16,
        "offset should be after durable header reserve"
    );
    assert_eq!(result.1, 512, "length should match");
}

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

#[test]
fn test_slot_directory_validate_empty() {
    let dir = SlotDirectory::new(PageSize::KiB16);
    dir.validate().expect("empty directory should validate");
}

#[test]
fn test_slot_directory_validate_with_slots() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    dir.allocate_slot(100).expect("alloc");
    dir.allocate_slot(200).expect("alloc");
    dir.allocate_slot(150).expect("alloc");

    dir.validate().expect("should validate with no overlaps");
}

#[test]
fn test_slot_directory_page_size_32kb() {
    let mut dir = SlotDirectory::new(PageSize::KiB32);

    for i in 0..5 {
        let size = 1000u16 + (i * 100u16);
        let result = dir.allocate_slot(size);
        assert!(result.is_ok(), "allocation {} should succeed", i);
    }

    assert_eq!(dir.slot_count(), 5);
    assert_eq!(dir.active_slot_count(), 5);
}

#[test]
fn test_slot_directory_16kb_max_slots() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let mut count = 0;
    for _ in 0..300 {
        match dir.allocate_slot(10) {
            Ok(_) => count += 1,
            Err(_) => break,
        }
    }

    assert!(count > 0, "should allocate some slots");
    assert!(count <= 256, "should not exceed max slots");
}

#[test]
fn test_slot_directory_serialize_deserialize() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);
    let _slot1 = dir.allocate_slot(100).expect("alloc 1");
    let _slot2 = dir.allocate_slot(200).expect("alloc 2");

    let mut page_data = vec![0u8; 16384];
    dir.serialize_to_page(&mut page_data).expect("serialize");

    let restored = SlotDirectory::from_page_data(PageSize::KiB16, &page_data).expect("deserialize");

    assert_eq!(restored.slot_count(), dir.slot_count());
    assert_eq!(restored.active_slot_count(), dir.active_slot_count());
}

#[test]
fn test_slot_directory_reuse_deleted_slot() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let slot1 = dir.allocate_slot(100).expect("alloc 1");
    dir.allocate_slot(200).expect("alloc 2");

    assert_eq!(dir.slot_count(), 2);

    dir.mark_deleted(slot1).expect("delete 1");
    assert_eq!(dir.active_slot_count(), 1);

    let slot3 = dir.allocate_slot(150).expect("alloc 3");
    assert_eq!(slot3.get(), 0, "should reuse first slot ID");
    assert_eq!(dir.slot_count(), 2, "total slots unchanged");
    assert_eq!(dir.active_slot_count(), 2, "active slots restored");
}

#[test]
fn test_slot_directory_fragmentation_pattern() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let large1 = dir.allocate_slot(500).expect("large 1");
    let small1 = dir.allocate_slot(50).expect("small 1");
    let large2 = dir.allocate_slot(500).expect("large 2");
    let small2 = dir.allocate_slot(50).expect("small 2");

    assert_eq!(dir.slot_count(), 4);

    dir.mark_deleted(small1).expect("delete small 1");
    dir.mark_deleted(small2).expect("delete small 2");

    assert_eq!(dir.active_slot_count(), 2);

    let freed = dir.compact();
    assert_eq!(freed, 50, "should remove only the deleted tail slot");

    assert!(dir.get_slot(large1).expect("get").is_some());
    assert!(dir.get_slot(large2).expect("get").is_some());
}

#[test]
fn test_slot_directory_no_tuple_overlap() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let slot1 = dir.allocate_slot(100).expect("alloc 1");
    let slot2 = dir.allocate_slot(150).expect("alloc 2");
    let slot3 = dir.allocate_slot(200).expect("alloc 3");

    let (o1, l1) = dir.get_slot(slot1).expect("get").expect("slot 1");
    let (o2, l2) = dir.get_slot(slot2).expect("get").expect("slot 2");
    let (o3, _l3) = dir.get_slot(slot3).expect("get").expect("slot 3");

    let end1 = o1 as u32 + l1 as u32;
    let end2 = o2 as u32 + l2 as u32;

    assert!(end1 <= o2 as u32, "slot 1 should not overlap slot 2");
    assert!(end2 <= o3 as u32, "slot 2 should not overlap slot 3");

    dir.validate().expect("should validate");
}

#[test]
fn test_slot_directory_active_count_accuracy() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    for _ in 0..10 {
        dir.allocate_slot(50).expect("alloc");
    }

    assert_eq!(dir.active_slot_count(), 10);

    dir.mark_deleted(SlotId::new(0)).expect("delete");
    dir.mark_deleted(SlotId::new(2)).expect("delete");
    dir.mark_deleted(SlotId::new(5)).expect("delete");

    assert_eq!(dir.active_slot_count(), 7, "should have 7 active slots");

    dir.compact();
    assert_eq!(
        dir.active_slot_count(),
        7,
        "active count unchanged after compact"
    );
}

#[test]
fn test_slot_directory_large_tuple() {
    let mut dir = SlotDirectory::new(PageSize::KiB32);

    let slot = dir.allocate_slot(8192).expect("allocate 8KB");
    let (offset, length) = dir.get_slot(slot).expect("get").expect("exists");
    assert_eq!(length, 8192);
    assert!(offset > 0);
}

#[test]
fn test_slot_directory_header_boundary() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let slot1 = dir.allocate_slot(100).expect("alloc");
    let (offset1, _length1) = dir.get_slot(slot1).expect("get").expect("exists");

    assert_eq!(
        offset1, HEADER_SIZE as u16,
        "first slot should start at the HeapPageV1 payload offset"
    );
}

#[test]
fn test_slot_directory_allocation_patterns() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let slots: Vec<_> = (0..10)
        .map(|i| {
            let size = if i % 2 == 0 { 200 } else { 50 };
            dir.allocate_slot(size).expect("alloc")
        })
        .collect();

    assert_eq!(dir.slot_count(), 10);
    assert_eq!(dir.active_slot_count(), 10);

    for slot_id in &slots {
        assert!(dir.get_slot(*slot_id).expect("get").is_some());
    }
}

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

#[test]
fn test_slot_directory_page_size_capacity() {
    let mut dir16 = SlotDirectory::new(PageSize::KiB16);
    let mut dir32 = SlotDirectory::new(PageSize::KiB32);

    let test_size = 5000u16;

    let result16 = dir16.allocate_slot(test_size);
    let result32 = dir32.allocate_slot(test_size);

    let free16 = dir16.free_space();
    let free32 = dir32.free_space();

    assert!(free32 > free16 || (result16.is_err() && result32.is_ok()));
}

#[test]
fn test_slot_directory_serialize_preserves_structure() {
    let mut dir = SlotDirectory::new(PageSize::KiB16);

    let slots: Vec<_> = (0..5)
        .map(|i| dir.allocate_slot(100 + (i * 50u16)).expect("alloc"))
        .collect();

    dir.mark_deleted(slots[1]).expect("delete");
    dir.mark_deleted(slots[3]).expect("delete");

    let mut page_data = vec![0u8; 16384];
    dir.serialize_to_page(&mut page_data).expect("serialize");

    let restored = SlotDirectory::from_page_data(PageSize::KiB16, &page_data).expect("deserialize");

    assert_eq!(restored.slot_count(), dir.slot_count());
    assert_eq!(restored.active_slot_count(), dir.active_slot_count());

    assert!(restored.get_slot(slots[0]).expect("get").is_some());
    assert!(restored.get_slot(slots[1]).expect("get").is_none());
    assert!(restored.get_slot(slots[2]).expect("get").is_some());
    assert!(restored.get_slot(slots[3]).expect("get").is_none());
    assert!(restored.get_slot(slots[4]).expect("get").is_some());
}
