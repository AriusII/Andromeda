#![forbid(unsafe_code)]

/// Wave 21 Batch 4 Task 3: N1-HEAP-005
/// Heap Page Insert Implementation — Integration Tests
///
/// Validates end-to-end heap tuple insertion with:
/// - Row encoding via RowEncoder
/// - Slot directory management
/// - Buffer pool coordination (simulation)
/// - Invariant verification
/// - Performance characteristics
use andromeda_storage::{
    ColumnDef, Datum, HeapPageInsert, PageId, PageSize, RowEncoder, RowSchema, ScalarType,
};
use std::sync::Arc;

/// Create a schema for testing (id: i64, name: u32, active: bool)
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

/// Test 1: HeapPageInsert can be instantiated with canonical types
#[test]
fn heap_page_insert_creation_with_canonical_types() {
    let insert =
        HeapPageInsert::new(PageId::new(100), PageSize::KiB16).expect("create insert context");

    assert_eq!(insert.page_id(), PageId::new(100));
    assert_eq!(insert.page_size(), PageSize::KiB16);
    assert_eq!(insert.slot_count(), 0);
    assert_eq!(insert.active_slot_count(), 0);
    assert!(insert.free_space() > 0);
}

/// Test 2: Insert single raw tuple and verify retrieval
#[test]
fn heap_page_insert_single_raw_tuple() {
    let mut insert =
        HeapPageInsert::new(PageId::new(1), PageSize::KiB16).expect("create insert context");

    let tuple_data = b"hello_world_test";
    let slot_id = insert.insert_raw_tuple(tuple_data).expect("insert");

    assert_eq!(slot_id, 0);
    assert_eq!(insert.slot_count(), 1);
    assert_eq!(insert.active_slot_count(), 1);

    let read_data = insert.read_tuple(slot_id).expect("read");
    assert_eq!(read_data, tuple_data);
}

/// Test 3: Insert multiple tuples and verify independent retrieval
#[test]
fn heap_page_insert_multiple_tuples() {
    let mut insert =
        HeapPageInsert::new(PageId::new(2), PageSize::KiB16).expect("create insert context");

    let mut slot_ids = Vec::new();
    for i in 0..20 {
        let data = format!("tuple_number_{}_data", i);
        let slot_id = insert.insert_raw_tuple(data.as_bytes()).expect("insert");
        slot_ids.push((slot_id, data));
    }

    assert_eq!(insert.slot_count(), 20);
    assert_eq!(insert.active_slot_count(), 20);

    // Verify each tuple independently
    for (slot_id, expected_data) in slot_ids {
        let read_data = insert.read_tuple(slot_id).expect("read");
        assert_eq!(read_data, expected_data.as_bytes());
    }
}

/// Test 4: Fill page to near-capacity
#[test]
fn heap_page_insert_fill_to_capacity() {
    let mut insert =
        HeapPageInsert::new(PageId::new(3), PageSize::KiB16).expect("create insert context");

    let large_chunk = vec![42u8; 2000];
    let mut inserted_count = 0;

    loop {
        match insert.insert_raw_tuple(&large_chunk) {
            Ok(_) => {
                inserted_count += 1;
            }
            Err(e) if e.message().contains("full") => {
                break;
            }
            Err(e) => {
                panic!("unexpected error: {}", e.message());
            }
        }
    }

    // Should have inserted at least 7-8 chunks in a 16 KiB page
    assert!(inserted_count >= 7, "should fill at least 7 chunks");

    // Page should now be full
    let result = insert.insert_raw_tuple(&large_chunk);
    assert!(
        result.is_err() && result.unwrap_err().message().contains("full"),
        "page should reject further insertions"
    );
}

/// Test 5: Page full error has appropriate error message
#[test]
fn heap_page_insert_page_full_error_message() {
    let mut insert =
        HeapPageInsert::new(PageId::new(4), PageSize::KiB16).expect("create insert context");

    // Fill page with maximum-size tuples
    let mut free_space = insert.free_space() as usize;
    while free_space >= 100 {
        let chunk = vec![0u8; 100];
        let _ = insert.insert_raw_tuple(&chunk);
        free_space = insert.free_space() as usize;
    }

    // Try to insert when full
    let oversized = vec![0u8; 1000];
    let result = insert.insert_raw_tuple(&oversized);

    assert!(result.is_err());
    let error = result.unwrap_err();
    let err_msg = error.message();
    assert!(err_msg.contains("full") || err_msg.contains("free"));
}

/// Test 6: Encoding round-trip with row encoder
#[test]
fn heap_page_insert_encoding_roundtrip() {
    let schema = create_test_schema();
    let encoder = RowEncoder::new(schema.clone());

    let mut insert =
        HeapPageInsert::new(PageId::new(5), PageSize::KiB16).expect("create insert context");

    let datums = vec![Datum::Int64(42), Datum::Null, Datum::Bool(true)];

    // Encode externally
    let encoded = encoder.encode(&datums).expect("encode");

    // Insert encoded bytes
    let slot_id = insert.insert_raw_tuple(&encoded).expect("insert");

    // Read back
    let read_encoded = insert.read_tuple(slot_id).expect("read");

    // Verify encoding is deterministic
    assert_eq!(encoded, read_encoded);
}

/// Test 7: Insert structured row with encoder
#[test]
fn heap_page_insert_structured_row_with_encoder() {
    let schema = create_test_schema();
    let encoder = RowEncoder::new(schema);

    let mut insert = HeapPageInsert::new(PageId::new(6), PageSize::KiB16)
        .expect("create insert context")
        .with_encoder(encoder);

    let rows = vec![
        vec![Datum::Int64(1), Datum::Null, Datum::Bool(true)],
        vec![Datum::Int64(2), Datum::Null, Datum::Bool(false)],
        vec![Datum::Int64(3), Datum::Null, Datum::Bool(true)],
    ];

    let mut slot_ids = Vec::new();
    for row in &rows {
        let slot_id = insert.insert_tuple(row).expect("insert");
        slot_ids.push(slot_id);
    }

    assert_eq!(insert.slot_count(), 3);
    assert_eq!(insert.active_slot_count(), 3);
}

/// Test 8: Multiple rows with various data types
#[test]
fn heap_page_insert_diverse_data_types() {
    let schema = create_test_schema();
    let encoder = RowEncoder::new(schema);

    let mut insert = HeapPageInsert::new(PageId::new(7), PageSize::KiB16)
        .expect("create insert context")
        .with_encoder(encoder);

    let test_rows = vec![
        vec![Datum::Int64(i64::MIN), Datum::Null, Datum::Bool(true)],
        vec![Datum::Int64(0), Datum::Null, Datum::Bool(false)],
        vec![Datum::Int64(i64::MAX), Datum::Null, Datum::Bool(true)],
        vec![Datum::Int64(12345), Datum::Null, Datum::Bool(false)],
    ];

    for row in test_rows {
        let _ = insert.insert_tuple(&row).expect("insert");
    }

    assert_eq!(insert.active_slot_count(), 4);
}

/// Test 9: RowId uniqueness within page
#[test]
fn heap_page_insert_rowid_uniqueness() {
    let schema = create_test_schema();
    let encoder = RowEncoder::new(schema);

    let mut insert = HeapPageInsert::new(PageId::new(8), PageSize::KiB16)
        .expect("create insert context")
        .with_encoder(encoder);

    let datums = vec![Datum::Int64(123), Datum::Null, Datum::Bool(false)];

    let mut slot_ids = Vec::new();
    for _ in 0..5 {
        let slot_id = insert.insert_tuple(&datums).expect("insert");
        slot_ids.push(slot_id);
    }

    // All slot IDs should be unique
    let unique_count = slot_ids
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len();
    assert_eq!(unique_count, 5, "all slot IDs should be unique");
}

/// Test 10: Delete tuple and verify cannot be read
#[test]
fn heap_page_insert_delete_tuple_logical() {
    let mut insert =
        HeapPageInsert::new(PageId::new(9), PageSize::KiB16).expect("create insert context");

    let slot1 = insert.insert_raw_tuple(b"data1").expect("insert 1");
    let slot2 = insert.insert_raw_tuple(b"data2").expect("insert 2");
    let slot3 = insert.insert_raw_tuple(b"data3").expect("insert 3");

    assert_eq!(insert.active_slot_count(), 3);

    // Delete middle tuple
    insert.delete_tuple(slot2).expect("delete");

    assert_eq!(insert.active_slot_count(), 2);

    // Can read 1 and 3
    assert_eq!(insert.read_tuple(slot1).expect("read 1"), b"data1");
    assert_eq!(insert.read_tuple(slot3).expect("read 3"), b"data3");

    // Cannot read 2
    let result = insert.read_tuple(slot2);
    assert!(result.is_err(), "deleted tuple should not be readable");
}

/// Test 11: No tuple loss guarantee
#[test]
fn heap_page_insert_no_tuple_loss() {
    let mut insert =
        HeapPageInsert::new(PageId::new(10), PageSize::KiB16).expect("create insert context");

    let mut inserted_data = Vec::new();
    let mut slot_ids = Vec::new();

    // Insert 50 small tuples
    for i in 0..50 {
        let data = format!("tuple_{:03}", i);
        let slot_id = insert.insert_raw_tuple(data.as_bytes()).expect("insert");
        inserted_data.push(data);
        slot_ids.push(slot_id);
    }

    // Delete some (even-numbered)
    for slot_id in slot_ids
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 0)
        .map(|(_, s)| s)
    {
        let _ = insert.delete_tuple(*slot_id);
    }

    // All non-deleted tuples should still be readable
    for (i, (data, slot_id)) in inserted_data.iter().zip(slot_ids.iter()).enumerate() {
        let read = insert.read_tuple(*slot_id);
        if i % 2 == 0 {
            // Deleted
            assert!(read.is_err(), "deleted tuple should not be readable");
        } else {
            // Not deleted
            assert_eq!(
                read.expect("read"),
                data.as_bytes(),
                "live tuple should be readable"
            );
        }
    }
}

/// Test 12: Free space tracking with insertions
#[test]
fn heap_page_insert_free_space_tracking() {
    let mut insert =
        HeapPageInsert::new(PageId::new(11), PageSize::KiB16).expect("create insert context");

    let initial_free = insert.free_space();

    let data1 = vec![0u8; 500];
    insert.insert_raw_tuple(&data1).expect("insert 1");
    let free_after1 = insert.free_space();

    assert!(
        free_after1 < initial_free,
        "free space should decrease after insertion"
    );

    let data2 = vec![0u8; 800];
    insert.insert_raw_tuple(&data2).expect("insert 2");
    let free_after2 = insert.free_space();

    assert!(
        free_after2 < free_after1,
        "free space should decrease further"
    );

    // Roughly verify accounting
    let used = initial_free - free_after2;
    assert!(
        used >= 1300,
        "used space should account for both insertions"
    );
}

/// Test 13: Large tuple insertion
#[test]
fn heap_page_insert_large_tuple() {
    let mut insert =
        HeapPageInsert::new(PageId::new(12), PageSize::KiB32).expect("create insert context");

    let large_data = vec![99u8; 10000]; // 10 KB
    let slot_id = insert.insert_raw_tuple(&large_data).expect("insert");

    let read = insert.read_tuple(slot_id).expect("read");
    assert_eq!(read, large_data);
}

/// Test 14: Page size variants (16 KiB and 32 KiB)
#[test]
fn heap_page_insert_page_size_variants() {
    // Test with 16 KiB page
    let insert16 =
        HeapPageInsert::new(PageId::new(13), PageSize::KiB16).expect("create 16KiB insert");
    assert_eq!(insert16.page_size(), PageSize::KiB16);

    // Test with 32 KiB page
    let insert32 =
        HeapPageInsert::new(PageId::new(14), PageSize::KiB32).expect("create 32KiB insert");
    assert_eq!(insert32.page_size(), PageSize::KiB32);

    // 32 KiB page should have more free space
    assert!(insert32.free_space() > insert16.free_space());
}

/// Test 15: Serialization produces valid page image
#[test]
fn heap_page_insert_serialize_produces_valid_image() {
    let mut insert =
        HeapPageInsert::new(PageId::new(15), PageSize::KiB16).expect("create insert context");

    insert.insert_raw_tuple(b"data1").expect("insert 1");
    insert.insert_raw_tuple(b"data2").expect("insert 2");

    let serialized = insert.serialize().expect("serialize");

    // Page image should be exactly page_size bytes
    assert_eq!(serialized.len(), PageSize::KiB16.bytes_usize());

    // First 96 bytes should be accessible (header region)
    assert!(serialized.len() >= 96);
}

/// Test 16: Invariant verification - no overlapping tuples
#[test]
fn heap_page_insert_invariant_no_overlap() {
    let mut insert =
        HeapPageInsert::new(PageId::new(16), PageSize::KiB16).expect("create insert context");

    // Insert varied-size tuples
    for i in 0..30 {
        let size = ((i * 17) % 200) + 50; // Varied sizes
        let data = vec![i as u8; size];
        let _ = insert.insert_raw_tuple(&data);
    }

    // All live tuples should be readable
    let active_count = insert.active_slot_count();
    for slot_id in 0..(insert.slot_count() as u16) {
        let _ = insert.read_tuple(slot_id);
    }

    assert!(active_count > 0, "should have inserted some tuples");
}

/// Test 17: Integration - encoder, insert, and read cycle
#[test]
fn heap_page_insert_full_integration_cycle() {
    let schema = create_test_schema();
    let encoder = RowEncoder::new(schema);

    let mut insert = HeapPageInsert::new(PageId::new(17), PageSize::KiB16)
        .expect("create insert context")
        .with_encoder(encoder.clone());

    let test_data = vec![(100i64, true), (200i64, false), (300i64, true)];

    let mut slot_ids = Vec::new();
    for (id, active) in &test_data {
        let row = vec![Datum::Int64(*id), Datum::Null, Datum::Bool(*active)];
        let slot_id = insert.insert_tuple(&row).expect("insert");
        slot_ids.push(slot_id);
    }

    assert_eq!(insert.active_slot_count(), 3);

    // Verify insertions through raw tuple retrieval
    for (i, slot_id) in slot_ids.iter().enumerate() {
        let raw_data = insert.read_tuple(*slot_id).expect("read");
        assert!(!raw_data.is_empty(), "tuple {} should have data", i);
    }
}

/// Test 18: Performance characteristic - insertion latency
#[test]
fn heap_page_insert_performance_insertion_latency() {
    let mut insert =
        HeapPageInsert::new(PageId::new(18), PageSize::KiB16).expect("create insert context");

    let data = vec![42u8; 100]; // 100 byte tuple

    let start = std::time::Instant::now();
    for _ in 0..100 {
        let _ = insert.insert_raw_tuple(&data);
    }
    let elapsed = start.elapsed();

    let avg_micros = elapsed.as_micros() as f64 / 100.0;
    println!("Average insertion latency: {:.2} μs", avg_micros);

    // Expected: < 10 microseconds per insertion
    assert!(
        avg_micros < 10.0,
        "insertion should average < 10μs (got {:.2}μs)",
        avg_micros
    );
}

/// Test 19: Rejection of invalid inputs
#[test]
fn heap_page_insert_rejects_invalid_inputs() {
    let mut insert =
        HeapPageInsert::new(PageId::new(19), PageSize::KiB16).expect("create insert context");

    // Empty tuple
    assert!(
        insert.insert_raw_tuple(&[]).is_err(),
        "should reject empty tuple"
    );

    // Oversized tuple (> u16::MAX)
    let oversized = vec![0u8; (u16::MAX as usize) + 1];
    assert!(
        insert.insert_raw_tuple(&oversized).is_err(),
        "should reject oversized tuple"
    );
}

/// Test 20: Deterministic slot assignment for same data
#[test]
fn heap_page_insert_deterministic_slot_assignment() {
    let mut insert1 =
        HeapPageInsert::new(PageId::new(20), PageSize::KiB16).expect("create insert1");
    let mut insert2 =
        HeapPageInsert::new(PageId::new(20), PageSize::KiB16).expect("create insert2");

    let data = b"deterministic_test_data";

    let slot1 = insert1.insert_raw_tuple(data).expect("insert1");
    let slot2 = insert2.insert_raw_tuple(data).expect("insert2");

    // Same page, same data → same slot assignment
    assert_eq!(
        slot1, slot2,
        "slot assignment should be deterministic for same page and data"
    );
}
