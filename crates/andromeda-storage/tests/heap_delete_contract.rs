//! Heap delete contract tests.
//!
//! Logical deletion, group deletion, and compaction.
//! Validates invariants and performance requirements.

#![forbid(unsafe_code)]

use andromeda_storage::{HeapPage, HeapVacuumMode, PageSize};

#[test]
fn test_heap_delete_single_tuple() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let slot_id = page.insert_tuple(b"test_data").expect("insert");
    page.delete_tuple(slot_id).expect("delete");

    // Verify tuple is deleted
    let result = page.read_tuple(slot_id);
    assert!(result.is_err());
    assert!(result.unwrap_err().message().contains("deleted"));
}

#[test]
fn test_heap_delete_nonexistent_slot() {
    let mut page = HeapPage::new(PageSize::KiB16);
    let result = page.delete_tuple(999);
    assert!(result.is_err());
}

#[test]
fn test_heap_delete_already_deleted() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let slot_id = page.insert_tuple(b"test").expect("insert");
    page.delete_tuple(slot_id).expect("first delete");

    let result = page.delete_tuple(slot_id);
    assert!(result.is_err());
    assert!(result.unwrap_err().message().contains("already deleted"));
}

#[test]
fn test_heap_delete_preserves_live() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let slot1 = page.insert_tuple(b"tuple1").expect("insert 1");
    let slot2 = page.insert_tuple(b"tuple2").expect("insert 2");
    let slot3 = page.insert_tuple(b"tuple3").expect("insert 3");

    page.delete_tuple(slot2).expect("delete 2");

    // Check live tuples still readable
    assert_eq!(page.read_tuple(slot1).unwrap(), b"tuple1");
    assert!(page.read_tuple(slot2).is_err());
    assert_eq!(page.read_tuple(slot3).unwrap(), b"tuple3");
}

#[test]
fn test_heap_delete_consecutive() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let ids: Vec<_> = (0..10)
        .map(|i| {
            page.insert_tuple(format!("tuple_{}", i).as_bytes())
                .expect("insert")
        })
        .collect();

    // Delete even-indexed tuples
    for i in (0..ids.len()).step_by(2) {
        page.delete_tuple(ids[i]).expect("delete");
    }

    // Verify deletion
    assert_eq!(page.live_row_count(), 5);
}

#[test]
fn test_heap_batch_delete_multiple() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let ids: Vec<_> = (0..10)
        .map(|i| {
            page.insert_tuple(format!("tuple_{}", i).as_bytes())
                .expect("insert")
        })
        .collect();

    let to_delete = vec![ids[1], ids[3], ids[5]];
    let count = page.mark_deleted_batch(&to_delete).expect("batch delete");

    assert_eq!(count, 3);
    assert_eq!(page.live_row_count(), 7);
}

#[test]
fn test_heap_batch_delete_out_of_range() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let slot1 = page.insert_tuple(b"data1").expect("insert");
    let _slot2 = page.insert_tuple(b"data2").expect("insert");

    let to_delete = vec![slot1, 999]; // 999 is out of range
    let result = page.mark_deleted_batch(&to_delete);

    assert!(result.is_err());
    // Verify all-or-nothing: slot1 should still be live
    assert_eq!(page.read_tuple(slot1).unwrap(), b"data1");
}

#[test]
fn test_heap_batch_delete_empty() {
    let mut page = HeapPage::new(PageSize::KiB16);
    page.insert_tuple(b"data").expect("insert");

    let count = page.mark_deleted_batch(&[]).expect("batch delete");
    assert_eq!(count, 0);
}

#[test]
fn test_heap_batch_delete_already_deleted() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let slot1 = page.insert_tuple(b"data1").expect("insert");
    let slot2 = page.insert_tuple(b"data2").expect("insert");

    page.delete_tuple(slot1).expect("delete slot1");

    let to_delete = vec![slot1, slot2]; // slot1 already deleted
    let result = page.mark_deleted_batch(&to_delete);

    assert!(result.is_err());
    // Verify all-or-nothing: slot2 should still be live
    assert_eq!(page.read_tuple(slot2).unwrap(), b"data2");
}

#[test]
fn test_has_deleted_slots_empty() {
    let page = HeapPage::new(PageSize::KiB16);
    assert!(!page.has_deleted_slots());
}

#[test]
fn test_has_deleted_slots_after_delete() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let slot = page.insert_tuple(b"data").expect("insert");
    assert!(!page.has_deleted_slots());

    page.delete_tuple(slot).expect("delete");
    assert!(page.has_deleted_slots());
}

#[test]
fn test_heap_compact_no_deleted() {
    let mut page = HeapPage::new(PageSize::KiB16);

    page.insert_tuple(b"data1").expect("insert");
    page.insert_tuple(b"data2").expect("insert");

    let reclaimed = page.compact_deleted().expect("compact");
    assert_eq!(reclaimed, 0);
}

#[test]
fn test_heap_compact_with_deleted() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let slot1 = page.insert_tuple(b"data1").expect("insert 1"); // 5 bytes
    let slot2 = page.insert_tuple(b"data2_longer").expect("insert 2"); // 12 bytes
    let slot3 = page.insert_tuple(b"data3").expect("insert 3"); // 5 bytes

    page.delete_tuple(slot2).expect("delete 2");

    let reclaimed = page.compact_deleted().expect("compact");
    assert_eq!(reclaimed, 12); // Reclaimed size of deleted tuple

    // Verify live data preserved
    assert_eq!(page.read_tuple(slot1).unwrap(), b"data1");
    assert_eq!(page.read_tuple(slot3).unwrap(), b"data3");

    // Verify deleted slot still deleted
    assert!(page.read_tuple(slot2).is_err());
}

#[test]
fn test_heap_compact_preserves_order() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let ids: Vec<_> = (0..5)
        .map(|i| {
            page.insert_tuple(format!("tuple_{:02}", i).as_bytes())
                .expect("insert")
        })
        .collect();

    // Delete middle tuple
    page.delete_tuple(ids[2]).expect("delete");
    page.compact_deleted().expect("compact");

    // Verify order preserved
    assert_eq!(page.read_tuple(ids[0]).unwrap(), b"tuple_00");
    assert_eq!(page.read_tuple(ids[1]).unwrap(), b"tuple_01");
    assert!(page.read_tuple(ids[2]).is_err()); // Still deleted
    assert_eq!(page.read_tuple(ids[3]).unwrap(), b"tuple_03");
    assert_eq!(page.read_tuple(ids[4]).unwrap(), b"tuple_04");
}

#[test]
fn test_heap_compact_data_integrity() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let ids: Vec<_> = (0..20)
        .map(|i| {
            let data = format!("data_block_{:03}", i);
            page.insert_tuple(data.as_bytes()).expect("insert")
        })
        .collect();

    // Delete multiple tuples
    for i in (0..ids.len()).step_by(3) {
        page.delete_tuple(ids[i]).expect("delete");
    }

    page.compact_deleted().expect("compact");

    // Verify remaining data is still readable and intact
    let mut found_count = 0;
    for (i, &slot_id) in ids.iter().enumerate() {
        if i % 3 != 0 {
            // Should be readable
            let data = page.read_tuple(slot_id).expect("read");
            assert!(!data.is_empty(), "live tuple should have data");
            found_count += 1;
        }
    }

    assert_eq!(found_count, page.live_row_count() as usize);
}

#[test]
fn test_heap_compact_multiple_deletions() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let ids: Vec<_> = (0..10)
        .map(|i| {
            page.insert_tuple(format!("data_{}", i).as_bytes())
                .expect("insert")
        })
        .collect();

    // Delete every other tuple
    for i in (0..ids.len()).step_by(2) {
        page.delete_tuple(ids[i]).expect("delete");
    }

    assert_eq!(page.live_row_count(), 5);

    let reclaimed = page.compact_deleted().expect("compact");
    assert!(reclaimed > 0);
    assert_eq!(page.live_row_count(), 5); // Live count unchanged
}

#[test]
fn test_heap_insert_after_delete() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let slot1 = page.insert_tuple(b"data1").expect("insert 1");
    page.delete_tuple(slot1).expect("delete");

    // Should still be able to insert (space available from deleted slot or new space)
    let slot2 = page.insert_tuple(b"data2_new").expect("insert 2");
    assert_eq!(page.read_tuple(slot2).unwrap(), b"data2_new");
}

#[test]
fn test_heap_scan_skips_deleted() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let ids: Vec<_> = (0..5)
        .map(|i| {
            page.insert_tuple(format!("tuple_{}", i).as_bytes())
                .expect("insert")
        })
        .collect();

    page.delete_tuple(ids[1]).expect("delete 1");
    page.delete_tuple(ids[3]).expect("delete 3");

    let scanned: Vec<_> = page.scan().collect::<Result<Vec<_>, _>>().expect("scan");

    assert_eq!(scanned.len(), 3);
    // Verify scan contains only live tuples
    for (_slot_id, data) in scanned {
        let s = String::from_utf8(data).unwrap();
        assert!(!s.contains("tuple_1") && !s.contains("tuple_3"));
    }
}

#[test]
fn test_heap_mixed_operations() {
    let mut page = HeapPage::new(PageSize::KiB16);

    // Insert phase
    let ids: Vec<_> = (0..20)
        .map(|i| {
            page.insert_tuple(format!("tuple_{:02}", i).as_bytes())
                .expect("insert")
        })
        .collect();

    // Single delete
    page.delete_tuple(ids[5]).expect("delete 5");

    // Delete a group of rows.
    let batch = vec![ids[10], ids[11], ids[12]];
    let count = page.mark_deleted_batch(&batch).expect("batch delete");
    assert_eq!(count, 3);

    // Verify live count
    let live_before = page.live_row_count();
    assert_eq!(live_before, 16); // 20 - 4 deleted

    // Compact
    let reclaimed = page.compact_deleted().expect("compact");
    assert!(reclaimed > 0);

    // Verify live count unchanged
    assert_eq!(page.live_row_count(), 16);

    // Verify remaining tuples readable
    for (i, &slot_id) in ids.iter().enumerate().take(20) {
        if i == 5 || i == 10 || i == 11 || i == 12 {
            assert!(page.read_tuple(slot_id).is_err());
        } else {
            assert!(page.read_tuple(slot_id).is_ok());
        }
    }
}

// Page Size Variants (2 tests)

#[test]
fn test_heap_delete_16kib() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let ids: Vec<_> = (0..50)
        .map(|_i| page.insert_tuple(&[0u8; 100]).expect("insert"))
        .collect();

    for i in (0..ids.len()).step_by(2) {
        page.delete_tuple(ids[i]).expect("delete");
    }

    let live = page.live_row_count();
    assert_eq!(live, 25);
}

#[test]
fn test_heap_delete_32kib() {
    let mut page = HeapPage::new(PageSize::KiB32);

    let ids: Vec<_> = (0..100)
        .map(|_i| page.insert_tuple(&[0u8; 100]).expect("insert"))
        .collect();

    for i in (0..ids.len()).step_by(2) {
        page.delete_tuple(ids[i]).expect("delete");
    }

    let live = page.live_row_count();
    assert_eq!(live, 50);
}

// Invariant Validation (2 tests)

#[test]
fn test_heap_compact_no_gaps() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let _ids: Vec<_> = (0..10)
        .map(|i| page.insert_tuple(&[i as u8; 100]).expect("insert"))
        .collect();

    // Delete some
    page.delete_tuple(1).expect("delete 1");
    page.delete_tuple(3).expect("delete 3");
    page.delete_tuple(7).expect("delete 7");

    page.compact_deleted().expect("compact");

    // Verify live tuples are still readable
    let live_count = page.live_row_count();
    assert_eq!(live_count, 7);
}

#[test]
fn test_heap_compact_idempotent() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let ids: Vec<_> = (0..10)
        .map(|i| {
            page.insert_tuple(format!("data_{}", i).as_bytes())
                .expect("insert")
        })
        .collect();

    page.delete_tuple(ids[2]).expect("delete");
    page.delete_tuple(ids[5]).expect("delete");

    let reclaimed1 = page.compact_deleted().expect("compact 1");
    let reclaimed2 = page.compact_deleted().expect("compact 2");

    assert_eq!(reclaimed1, reclaimed2);
}

#[test]
fn test_heap_compact_preserves_slot_ids_and_scan_ids() {
    let mut page = HeapPage::new(PageSize::KiB16);

    let slot0 = page.insert_tuple(b"row-0").expect("insert 0");
    let slot1 = page.insert_tuple(b"row-1-deleted").expect("insert 1");
    let slot2 = page.insert_tuple(b"row-2").expect("insert 2");
    let slot3 = page.insert_tuple(b"row-3-deleted").expect("insert 3");
    let slot4 = page.insert_tuple(b"row-4").expect("insert 4");

    page.delete_tuple(slot1).expect("delete 1");
    page.delete_tuple(slot3).expect("delete 3");

    let reclaimed = page.compact_deleted().expect("compact");

    assert_eq!((slot0, slot1, slot2, slot3, slot4), (0, 1, 2, 3, 4));
    assert_eq!(page.slot_count(), 5, "compaction must not renumber slots");
    assert_eq!(
        reclaimed,
        b"row-1-deleted".len() as u16 + b"row-3-deleted".len() as u16
    );
    assert_eq!(page.read_tuple(slot0).expect("read 0"), b"row-0");
    assert!(page.read_tuple(slot1).is_err());
    assert_eq!(page.read_tuple(slot2).expect("read 2"), b"row-2");
    assert!(page.read_tuple(slot3).is_err());
    assert_eq!(page.read_tuple(slot4).expect("read 4"), b"row-4");

    let scanned_ids: Vec<u16> = page
        .scan()
        .collect::<Result<Vec<_>, _>>()
        .expect("scan")
        .into_iter()
        .map(|(slot_id, _)| slot_id)
        .collect();
    assert_eq!(scanned_ids, vec![slot0, slot2, slot4]);
}

// Heap Physical Vacuum Planning Contract

#[test]
fn test_heap_vacuum_plan_empty_page_has_no_apply_requirements() {
    let page = HeapPage::new(PageSize::KiB16);

    let plan = page.vacuum_plan(HeapVacuumMode::OnlineStableRowIds);

    assert!(plan.is_empty());
    assert!(plan.preserves_row_ids);
    assert!(!plan.row_id_remap_allowed);
    assert!(!plan.requires_mvcc_gc_proof);
    assert!(!plan.requires_wal_redo);
    assert!(!plan.durable_format_change);
}

#[test]
fn test_heap_vacuum_plan_stable_rowids_identifies_deleted_and_moved_live_slots() {
    let mut page = HeapPage::new(PageSize::KiB16);
    let slot0 = page.insert_tuple(b"aaaaa").expect("insert 0");
    let slot1 = page.insert_tuple(b"bbb").expect("insert 1");
    let slot2 = page.insert_tuple(b"cccc").expect("insert 2");
    let slot3 = page.insert_tuple(b"d").expect("insert 3");

    page.delete_tuple(slot1).expect("delete middle slot");

    let plan = page.vacuum_plan(HeapVacuumMode::OnlineStableRowIds);

    assert_eq!(slot0, 0);
    assert_eq!(slot1, 1);
    assert_eq!(slot2, 2);
    assert_eq!(slot3, 3);
    assert_eq!(plan.candidate_deleted_slots, vec![slot1]);
    assert_eq!(plan.live_slots_rewritten, vec![slot2, slot3]);
    assert_eq!(plan.bytes_reclaimable, 3);
    assert!(plan.preserves_row_ids);
    assert!(!plan.row_id_remap_allowed);
    assert!(plan.requires_mvcc_gc_proof);
    assert!(plan.requires_wal_redo);
    assert!(!plan.durable_format_change);
}

#[test]
fn test_heap_vacuum_plan_offline_mode_advertises_remap_requirement_not_format_change() {
    let mut page = HeapPage::new(PageSize::KiB16);
    let slot0 = page.insert_tuple(b"live").expect("insert 0");
    let slot1 = page.insert_tuple(b"dead").expect("insert 1");
    page.delete_tuple(slot0).expect("delete first slot");

    let plan = page.vacuum_plan(HeapVacuumMode::OfflineRewriteAllowRowIdRemap);

    assert_eq!(slot1, 1);
    assert_eq!(plan.candidate_deleted_slots, vec![slot0]);
    assert_eq!(plan.live_slots_rewritten, vec![slot1]);
    assert_eq!(plan.bytes_reclaimable, 4);
    assert!(!plan.preserves_row_ids);
    assert!(plan.row_id_remap_allowed);
    assert!(plan.requires_mvcc_gc_proof);
    assert!(plan.requires_wal_redo);
    assert!(!plan.durable_format_change);
}
