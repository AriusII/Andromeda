//! Comprehensive tests for the Heap Page Engine (N1-HEAP-010)
//!
//! Tests cover:
//! - Basic insert/read/delete operations
//! - Multiple concurrent operations
//! - Page capacity limits
//! - Slot directory management
//! - Row encoding/decoding roundtrips
//! - Error conditions

#[cfg(test)]
mod heap_engine_tests {
    use andromeda_storage::{
        ColumnDef, Datum, HeapPage, PageSize, RowEncoder, RowSchema, ScalarType, SlotEntry,
    };
    use std::sync::Arc;

    /// Test basic insert and read
    #[test]
    fn test_heap_insert_read() {
        let mut page = HeapPage::new(PageSize::KiB16);
        let tuple = b"test_data_123";

        let slot_id = page.insert_tuple(tuple).expect("insert_tuple failed");
        assert_eq!(slot_id, 0);

        let read_data = page.read_tuple(slot_id).expect("read_tuple failed");
        assert_eq!(read_data, tuple);
    }

    /// Test basic delete operation
    #[test]
    fn test_heap_delete() {
        let mut page = HeapPage::new(PageSize::KiB16);
        let tuple = b"data_to_delete";

        let slot_id = page.insert_tuple(tuple).expect("insert_tuple failed");

        page.delete_tuple(slot_id).expect("delete_tuple failed");

        let result = page.read_tuple(slot_id);
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("deleted"));
    }

    /// Test delete error on already-deleted slot
    #[test]
    fn test_heap_double_delete_error() {
        let mut page = HeapPage::new(PageSize::KiB16);
        let tuple = b"data";

        let slot_id = page.insert_tuple(tuple).expect("insert failed");
        page.delete_tuple(slot_id).expect("first delete failed");

        let result = page.delete_tuple(slot_id);
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("already deleted"));
    }

    /// Test multiple sequential inserts
    #[test]
    fn test_heap_multiple_inserts() {
        let mut page = HeapPage::new(PageSize::KiB16);

        let mut slot_ids = Vec::new();
        for i in 0..10 {
            let tuple = format!("tuple_{:02}", i);
            let slot_id = page.insert_tuple(tuple.as_bytes()).expect("insert failed");
            slot_ids.push(slot_id);

            assert_eq!(slot_id, i as u16);
        }

        // Verify all tuples readable
        for (i, &slot_id) in slot_ids.iter().enumerate() {
            let expected = format!("tuple_{:02}", i);
            let data = page.read_tuple(slot_id).expect("read failed");
            assert_eq!(data, expected.as_bytes());
        }
    }

    /// Test live_row_count with deletions
    #[test]
    fn test_heap_live_row_count() {
        let mut page = HeapPage::new(PageSize::KiB16);

        for i in 0..5 {
            page.insert_tuple(format!("row_{}", i).as_bytes())
                .expect("insert failed");
        }

        assert_eq!(page.live_row_count(), 5);

        page.delete_tuple(0).expect("delete 0 failed");
        assert_eq!(page.live_row_count(), 4);

        page.delete_tuple(2).expect("delete 2 failed");
        assert_eq!(page.live_row_count(), 3);
    }

    /// Test scan operation
    #[test]
    fn test_heap_scan() {
        let mut page = HeapPage::new(PageSize::KiB16);

        let tuples = ["row_0", "row_1", "row_2", "row_3", "row_4"];
        for tuple_str in tuples.iter() {
            page.insert_tuple(tuple_str.as_bytes())
                .expect("insert failed");
        }

        let scanned: Vec<_> = page
            .scan()
            .collect::<Result<Vec<_>, _>>()
            .expect("scan failed");

        assert_eq!(scanned.len(), 5);
        for (i, (slot_id, data)) in scanned.iter().enumerate() {
            assert_eq!(*slot_id as usize, i);
            assert_eq!(data, tuples[i].as_bytes());
        }
    }

    /// Test scan skips deleted slots
    #[test]
    fn test_heap_scan_with_deletions() {
        let mut page = HeapPage::new(PageSize::KiB16);

        for i in 0..5 {
            page.insert_tuple(format!("row_{}", i).as_bytes())
                .expect("insert failed");
        }

        // Delete slots 1 and 3
        page.delete_tuple(1).expect("delete failed");
        page.delete_tuple(3).expect("delete failed");

        let scanned: Vec<_> = page
            .scan()
            .collect::<Result<Vec<_>, _>>()
            .expect("scan failed");

        assert_eq!(scanned.len(), 3);

        // Verify correct slots remain
        let remaining_ids: Vec<_> = scanned.iter().map(|(id, _)| *id).collect();
        assert_eq!(remaining_ids, vec![0, 2, 4]);
    }

    /// Test update operation
    #[test]
    fn test_heap_update() {
        let mut page = HeapPage::new(PageSize::KiB16);

        let slot_id = page.insert_tuple(b"original_value").expect("insert failed");

        // Update should delete old and insert new
        let new_slot = page
            .update_tuple(slot_id, b"updated_value")
            .expect("update failed");

        // Old slot should be deleted
        assert!(page.read_tuple(slot_id).is_err());

        // New slot should have updated data
        let data = page.read_tuple(new_slot).expect("read failed");
        assert_eq!(data, b"updated_value");
    }

    /// Test update same slot_id boundary
    #[test]
    fn test_heap_update_creates_new_slot() {
        let mut page = HeapPage::new(PageSize::KiB16);

        page.insert_tuple(b"data1").expect("insert 1 failed");
        let slot_id = page.insert_tuple(b"data2").expect("insert 2 failed");
        page.insert_tuple(b"data3").expect("insert 3 failed");

        let new_slot = page
            .update_tuple(slot_id, b"updated")
            .expect("update failed");

        // New slot should have higher ID than original
        assert!(new_slot > slot_id);

        // Original should be deleted
        assert!(page.read_tuple(slot_id).is_err());
    }

    /// Test page full error
    #[test]
    fn test_heap_page_full() {
        let mut page = HeapPage::new(PageSize::KiB16);

        // Fill with 2 KB tuples
        let large_tuple = vec![0x42u8; 2000];

        let mut inserted = 0;
        loop {
            match page.insert_tuple(&large_tuple) {
                Ok(_) => inserted += 1,
                Err(e) => {
                    assert!(e.message().contains("page full"));
                    break;
                },
            }
        }

        // Should have inserted at least one
        assert!(inserted > 0);

        // Next insert should fail
        assert!(page.insert_tuple(&large_tuple).is_err());
    }

    /// Test invalid slot ID error
    #[test]
    fn test_heap_invalid_slot_id() {
        let mut page = HeapPage::new(PageSize::KiB16);

        page.insert_tuple(b"data").expect("insert failed");

        let result = page.read_tuple(999);
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("out of range"));
    }

    /// Test slot entry serialization
    #[test]
    fn test_slot_entry_roundtrip() {
        let original = SlotEntry::new(1234, 567);
        let bytes = original.to_bytes();
        let deserialized = SlotEntry::from_bytes(bytes);

        assert_eq!(original, deserialized);
    }

    /// Test slot entry deletion flag
    #[test]
    fn test_slot_entry_deletion_flag() {
        let mut entry = SlotEntry::new(100, 50);
        assert!(!entry.is_deleted());
        assert_eq!(entry.offset_if_live(), Some(100));

        entry.mark_deleted();
        assert!(entry.is_deleted());
        assert_eq!(entry.offset_if_live(), None);
        assert_eq!(entry.offset_if_live(), None); // Offset cleared from public live view
    }

    /// Test tuple too large error
    #[test]
    fn test_heap_tuple_too_large() {
        let mut page = HeapPage::new(PageSize::KiB16);

        // Try to insert tuple larger than u16::MAX
        let huge_tuple = vec![0u8; (u16::MAX as usize) + 1];
        let result = page.insert_tuple(&huge_tuple);

        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("too large"));
    }

    /// Test row encoder with fixed-width types
    #[test]
    fn test_row_encoder_fixed_types() {
        let schema = Arc::new(
            RowSchema::new(vec![
                ColumnDef {
                    name: "id".to_string(),
                    ordinal: 0,
                    scalar_type: ScalarType::Int64,
                    nullable: false,
                },
                ColumnDef {
                    name: "active".to_string(),
                    ordinal: 1,
                    scalar_type: ScalarType::Bool,
                    nullable: true,
                },
            ])
            .expect("schema failed"),
        );

        let encoder = RowEncoder::new(schema);
        let values = vec![Datum::Int64(42), Datum::Bool(true)];

        let encoded = encoder.encode(&values).expect("encode failed");
        let decoded = encoder.decode(&encoded).expect("decode failed");

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0], Datum::Int64(42));
        assert_eq!(decoded[1], Datum::Bool(true));
    }

    /// Test row encoder with null values
    #[test]
    fn test_row_encoder_nulls() {
        let schema = Arc::new(
            RowSchema::new(vec![
                ColumnDef {
                    name: "col1".to_string(),
                    ordinal: 0,
                    scalar_type: ScalarType::Int32,
                    nullable: true,
                },
                ColumnDef {
                    name: "col2".to_string(),
                    ordinal: 1,
                    scalar_type: ScalarType::Bool,
                    nullable: true,
                },
            ])
            .expect("schema failed"),
        );

        let encoder = RowEncoder::new(schema);
        let values = vec![Datum::Null, Datum::Bool(false)];

        let encoded = encoder.encode(&values).expect("encode failed");
        let decoded = encoder.decode(&encoded).expect("decode failed");

        assert_eq!(decoded[0], Datum::Null);
        assert_eq!(decoded[1], Datum::Bool(false));
    }

    /// Test row encoder mismatch error
    #[test]
    fn test_row_encoder_mismatch() {
        let schema = Arc::new(
            RowSchema::new(vec![ColumnDef {
                name: "id".to_string(),
                ordinal: 0,
                scalar_type: ScalarType::Int64,
                nullable: false,
            }])
            .expect("schema failed"),
        );

        let encoder = RowEncoder::new(schema);

        // Wrong number of values
        let values = vec![Datum::Int64(1), Datum::Int64(2)];
        let result = encoder.encode(&values);

        assert!(result.is_err());
    }

    /// Test Datum byte_length
    #[test]
    fn test_datum_byte_lengths() {
        assert_eq!(Datum::Int8(0).byte_length().unwrap(), 1);
        assert_eq!(Datum::Int16(0).byte_length().unwrap(), 2);
        assert_eq!(Datum::Int32(0).byte_length().unwrap(), 4);
        assert_eq!(Datum::Int64(0).byte_length().unwrap(), 8);
        assert_eq!(Datum::Bool(true).byte_length().unwrap(), 1);
        assert_eq!(Datum::Float32(0.0).byte_length().unwrap(), 4);
        assert_eq!(Datum::Float64(0.0).byte_length().unwrap(), 8);
        assert_eq!(Datum::Bytes(vec![1, 2, 3]).byte_length().unwrap(), 3);
        assert_eq!(Datum::Null.byte_length().unwrap(), 0);
    }

    /// Test Datum encode/decode roundtrip for various types
    #[test]
    fn test_datum_roundtrips() {
        // Int64
        let d1 = Datum::Int64(-1234567890);
        let encoded1 = d1.encode().expect("encode failed");
        let decoded1 = Datum::decode_scalar(ScalarType::Int64, &encoded1).expect("decode failed");
        assert_eq!(d1, decoded1);

        // Bool
        let d2 = Datum::Bool(true);
        let encoded2 = d2.encode().expect("encode failed");
        let decoded2 = Datum::decode_scalar(ScalarType::Bool, &encoded2).expect("decode failed");
        assert_eq!(d2, decoded2);

        // Float64
        let d3 = Datum::Float64(std::f64::consts::PI);
        let encoded3 = d3.encode().expect("encode failed");
        let decoded3 = Datum::decode_scalar(ScalarType::Float64, &encoded3).expect("decode failed");
        assert_eq!(d3, decoded3);

        // UInt32
        let d4 = Datum::UInt32(999999);
        let encoded4 = d4.encode().expect("encode failed");
        let decoded4 = Datum::decode_scalar(ScalarType::UInt32, &encoded4).expect("decode failed");
        assert_eq!(d4, decoded4);
    }

    /// Test page size management
    #[test]
    fn test_heap_different_page_sizes() {
        let mut page_16k = HeapPage::new(PageSize::KiB16);
        let mut page_32k = HeapPage::new(PageSize::KiB32);

        let tuple = b"test_data";

        let slot_16 = page_16k.insert_tuple(tuple).expect("16k insert failed");
        let slot_32 = page_32k.insert_tuple(tuple).expect("32k insert failed");

        assert_eq!(
            page_16k.read_tuple(slot_16).expect("16k read failed"),
            tuple
        );
        assert_eq!(
            page_32k.read_tuple(slot_32).expect("32k read failed"),
            tuple
        );

        // 32 KB page should have more free space
        let _schema_16 = RowSchema::new(vec![ColumnDef {
            name: "data".to_string(),
            ordinal: 0,
            scalar_type: ScalarType::Bool,
            nullable: false,
        }])
        .expect("schema failed");

        let _schema_32 = RowSchema::new(vec![ColumnDef {
            name: "data".to_string(),
            ordinal: 0,
            scalar_type: ScalarType::Bool,
            nullable: false,
        }])
        .expect("schema failed");

        // Just verify pages work with different sizes
        assert_eq!(page_16k.page_size(), PageSize::KiB16);
        assert_eq!(page_32k.page_size(), PageSize::KiB32);
    }

    /// Test slot_count includes deleted slots
    #[test]
    fn test_heap_slot_count_includes_deleted() {
        let mut page = HeapPage::new(PageSize::KiB16);

        page.insert_tuple(b"row0").expect("insert failed");
        page.insert_tuple(b"row1").expect("insert failed");
        page.insert_tuple(b"row2").expect("insert failed");

        assert_eq!(page.slot_count(), 3);

        page.delete_tuple(1).expect("delete failed");

        // slot_count should still be 3 (includes deleted)
        assert_eq!(page.slot_count(), 3);
        // but live_row_count should be 2
        assert_eq!(page.live_row_count(), 2);
    }
}
