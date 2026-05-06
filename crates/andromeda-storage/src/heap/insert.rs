use andromeda_core::AndromedaResult;

use crate::PageSize;
use crate::heap_row_encoder::{Datum, RowEncoder};

use super::{HeapPage, SlotEntry, heap_error, slot_directory};

impl HeapPage {
    pub fn insert_tuple(&mut self, tuple: &[u8]) -> AndromedaResult<u16> {
        if tuple.len() > u16::MAX as usize {
            return Err(heap_error("tuple too large: exceeds u16::MAX"));
        }

        let tuple_len = tuple.len() as u16;
        let free_space = self.compute_free_space_bytes();
        let needed = tuple_len as usize + SlotEntry::SIZE;

        if free_space < needed {
            return Err(heap_error(format!(
                "heap page full: need {} bytes, have {} bytes",
                needed, free_space
            )));
        }

        let next_tuple_offset = self
            .slot_directory
            .iter()
            .filter(|entry| !entry.is_deleted())
            .map(|entry| entry.offset as usize + entry.length as usize)
            .max()
            .unwrap_or(super::HEAP_PAGE_V1_HEADER_SIZE);
        let insert_offset = next_tuple_offset as u16;

        let offset_usize = insert_offset as usize;
        self.data[offset_usize..offset_usize + tuple_len as usize].copy_from_slice(tuple);

        let slot_id = self.slot_directory.len() as u16;
        self.slot_directory
            .push(SlotEntry::new(insert_offset, tuple_len));

        Ok(slot_id)
    }

    pub fn update_tuple(&mut self, slot_id: u16, new_tuple: &[u8]) -> AndromedaResult<u16> {
        self.delete_tuple(slot_id)?;
        self.insert_tuple(new_tuple)
    }
}

#[derive(Debug)]
pub struct HeapPageInsert {
    page_id: crate::PageId,
    page_size: PageSize,
    data: Vec<u8>,
    slot_directory: slot_directory::SlotDirectory,
    row_encoder: Option<RowEncoder>,
}

impl HeapPageInsert {
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

    pub fn with_encoder(mut self, encoder: RowEncoder) -> Self {
        self.row_encoder = Some(encoder);
        self
    }

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
        let free_space = self.slot_directory.free_space();
        let needed = tuple_len as usize + SlotEntry::SIZE;

        if free_space < needed as u16 {
            return Err(heap_error(format!(
                "page full: need {} bytes, have {} bytes",
                needed, free_space as usize
            )));
        }

        let slot_id = self.slot_directory.allocate_slot(tuple_len)?;
        let (offset, _) = self
            .slot_directory
            .get_slot(slot_id)?
            .ok_or_else(|| heap_error("slot allocation inconsistency"))?;

        let offset_usize = offset as usize;
        if offset_usize + tuple_bytes.len() > self.data.len() {
            return Err(heap_error(
                "page layout violation: tuple exceeds page bounds",
            ));
        }

        self.data[offset_usize..offset_usize + tuple_bytes.len()].copy_from_slice(tuple_bytes);
        Ok(slot_id.get())
    }

    pub fn insert_tuple(&mut self, datums: &[Datum]) -> AndromedaResult<u16> {
        let encoder = self
            .row_encoder
            .as_ref()
            .ok_or_else(|| heap_error("row encoder not configured"))?;
        let encoded = encoder.encode(datums)?;
        self.insert_raw_tuple(&encoded)
    }

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

    pub fn delete_tuple(&mut self, slot_id: u16) -> AndromedaResult<()> {
        let slot = slot_directory::SlotId::new(slot_id);
        self.slot_directory.mark_deleted(slot)?;
        Ok(())
    }

    pub fn page_id(&self) -> crate::PageId {
        self.page_id
    }

    pub fn page_size(&self) -> PageSize {
        self.page_size
    }

    pub fn slot_count(&self) -> usize {
        self.slot_directory.slot_count()
    }

    pub fn active_slot_count(&self) -> usize {
        self.slot_directory.active_slot_count()
    }

    pub fn free_space(&self) -> u16 {
        self.slot_directory.free_space()
    }

    pub fn serialize(&mut self) -> AndromedaResult<Vec<u8>> {
        self.slot_directory.serialize_to_page(&mut self.data)?;
        Ok(self.data.clone())
    }
}

#[cfg(test)]
mod heap_insert_tests {
    use std::sync::Arc;

    use crate::heap_row_encoder::{ColumnDef, RowSchema, ScalarType};

    use super::*;

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
        let result = insert.insert_raw_tuple(&large_tuple);
        assert!(
            result.is_err() && result.expect_err("should fail").message().contains("full"),
            "page should be full"
        );
    }

    #[test]
    fn test_insert_zero_length_rejected() {
        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context");

        let result = insert.insert_raw_tuple(&[]);
        assert!(result.is_err());
        assert!(result.expect_err("should fail").message().contains("empty"));
    }

    #[test]
    fn test_insert_oversized_tuple_rejected() {
        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context");

        let oversized = vec![0u8; (u16::MAX as usize) + 1];
        let result = insert.insert_raw_tuple(&oversized);
        assert!(result.is_err());
        assert!(
            result
                .expect_err("should fail")
                .message()
                .contains("too large")
        );
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
        assert!(
            result
                .expect_err("should fail")
                .message()
                .contains("encoder")
        );
    }

    #[test]
    fn test_delete_tuple_logical() {
        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context");

        let slot_id = insert.insert_raw_tuple(b"test").expect("insert");
        assert_eq!(insert.active_slot_count(), 1);

        insert.delete_tuple(slot_id).expect("delete");
        assert_eq!(insert.active_slot_count(), 0);

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

        assert_eq!(insert.read_tuple(id1).expect("read 1"), b"tuple1");
        assert_eq!(insert.read_tuple(id3).expect("read 3"), b"tuple3");
        assert!(insert.read_tuple(id2).is_err());
    }

    #[test]
    fn test_encoding_roundtrip() {
        let schema = create_test_schema();
        let encoder = RowEncoder::new(schema.clone());

        let datums = vec![Datum::Int64(12345), Datum::Null, Datum::Bool(false)];

        let encoded = encoder.encode(&datums).expect("encode");
        let mut insert = HeapPageInsert::new(crate::PageId::new(1), PageSize::KiB16)
            .expect("create insert context");

        let slot_id = insert.insert_raw_tuple(&encoded).expect("insert");
        let decoded_bytes = insert.read_tuple(slot_id).expect("read");

        assert_eq!(encoded, decoded_bytes);
    }

    #[test]
    fn test_rowid_determinism() {
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
