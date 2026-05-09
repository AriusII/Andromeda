use andromeda_error::AndromedaResult;
use andromeda_storage_page::{PageId, PageSize};
use andromeda_wal::Lsn;

use super::{
    Datum, HeapPage, HeapRowRedoPayloadV1, ProductStockRow, RowEncoder, SlotEntry, heap_error,
    heap_page_v1_validate_format_guard, product_stock_row_encoder,
    slot_directory::{SlotDirectory, SlotId},
    tuple_layout::{checked_tuple_len, read_tuple_bytes, tuple_insert_space, write_tuple_bytes},
};

impl HeapPage {
    pub fn insert_tuple(&mut self, tuple: &[u8]) -> AndromedaResult<u16> {
        let tuple_len = checked_tuple_len(tuple, None, |_| {
            "tuple too large: exceeds u16::MAX".to_string()
        })?;
        let free_space = self.compute_free_space_bytes();
        let needed = tuple_insert_space(tuple_len)?;

        if free_space < needed {
            return Err(heap_error(format!(
                "heap page full: need {} bytes, have {} bytes",
                needed, free_space
            )));
        }

        let insert_offset = self.next_tuple_offset() as u16;
        write_tuple_bytes(
            &mut self.data,
            insert_offset,
            tuple,
            "page layout violation: tuple exceeds page bounds",
        )?;

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

/// Heap page insert context for raw tuple bytes.
///
/// Callers that expose typed rows or WAL redo envelopes must stage their typed
/// material outside this owner crate and append durable WAL before publishing
/// the serialized page image.
#[derive(Debug)]
pub struct HeapPageInsert {
    page_id: PageId,
    page_size: PageSize,
    data: Vec<u8>,
    slot_directory: SlotDirectory,
    row_encoder: Option<RowEncoder>,
}

impl HeapPageInsert {
    pub fn new(page_id: PageId, page_size: PageSize) -> AndromedaResult<Self> {
        heap_page_v1_validate_format_guard()?;

        if page_size.bytes() < 4096 {
            return Err(heap_error("page size must be >= 4 KiB"));
        }

        let size = page_size.bytes_usize();
        Ok(Self {
            page_id,
            page_size,
            data: vec![0u8; size],
            slot_directory: SlotDirectory::new(page_size),
            row_encoder: None,
        })
    }

    pub fn for_product_stock(page_id: PageId, page_size: PageSize) -> AndromedaResult<Self> {
        Ok(Self::new(page_id, page_size)?.with_encoder(product_stock_row_encoder()?))
    }

    pub fn with_encoder(mut self, encoder: RowEncoder) -> Self {
        self.row_encoder = Some(encoder);
        self
    }

    pub fn insert_raw_tuple(&mut self, tuple_bytes: &[u8]) -> AndromedaResult<u16> {
        let tuple_len = checked_tuple_len(tuple_bytes, Some("tuple must not be empty"), |len| {
            format!("tuple too large: {} bytes (max {})", len, u16::MAX)
        })?;
        let free_space = usize::from(self.slot_directory.free_space());
        let needed = tuple_insert_space(tuple_len)?;

        if free_space < needed {
            return Err(heap_error(format!(
                "page full: need {} bytes, have {} bytes",
                needed, free_space
            )));
        }

        let slot_id = self.slot_directory.allocate_slot(tuple_len)?;
        let (offset, _) = self
            .slot_directory
            .get_slot(slot_id)?
            .ok_or_else(|| heap_error("slot allocation inconsistency"))?;

        write_tuple_bytes(
            &mut self.data,
            offset,
            tuple_bytes,
            "page layout violation: tuple exceeds page bounds",
        )?;
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

    pub fn insert_product_stock(
        &mut self,
        row: ProductStockRow,
    ) -> AndromedaResult<ProductStockHeapInsert> {
        let tuple = row.encode()?;
        let slot_id = self.insert_raw_tuple(&tuple)?;
        Ok(ProductStockHeapInsert {
            page_id: self.page_id,
            page_size: self.page_size,
            slot_id,
            row,
            tuple,
        })
    }

    pub fn read_tuple(&self, slot_id: u16) -> AndromedaResult<Vec<u8>> {
        let slot = SlotId::new(slot_id);
        let (offset, length) = self
            .slot_directory
            .get_slot(slot)?
            .ok_or_else(|| heap_error("slot deleted or not found"))?;

        read_tuple_bytes(
            &self.data,
            offset,
            length,
            "page corrupted: slot offset out of bounds",
            "page corrupted: slot offset out of bounds",
        )
    }

    pub fn read_product_stock(&self, slot_id: u16) -> AndromedaResult<ProductStockRow> {
        ProductStockRow::decode(&self.read_tuple(slot_id)?)
    }

    pub fn delete_tuple(&mut self, slot_id: u16) -> AndromedaResult<()> {
        let slot = SlotId::new(slot_id);
        self.slot_directory.mark_deleted(slot)?;
        Ok(())
    }

    pub const fn page_id(&self) -> PageId {
        self.page_id
    }

    pub const fn page_size(&self) -> PageSize {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductStockHeapInsert {
    page_id: PageId,
    page_size: PageSize,
    slot_id: u16,
    row: ProductStockRow,
    tuple: Vec<u8>,
}

impl ProductStockHeapInsert {
    pub const fn page_id(&self) -> PageId {
        self.page_id
    }

    pub const fn page_size(&self) -> PageSize {
        self.page_size
    }

    pub const fn slot_id(&self) -> u16 {
        self.slot_id
    }

    pub const fn row(&self) -> ProductStockRow {
        self.row
    }

    pub fn tuple(&self) -> &[u8] {
        &self.tuple
    }

    pub fn row_insert_redo_payload(
        &self,
        expected_previous_page_lsn: Lsn,
        resulting_page_lsn: Lsn,
    ) -> AndromedaResult<HeapRowRedoPayloadV1> {
        HeapRowRedoPayloadV1::row_insert(
            self.page_id(),
            self.page_size(),
            self.slot_id(),
            expected_previous_page_lsn,
            resulting_page_lsn,
            self.tuple().to_vec(),
        )
        .map_err(|error| heap_error(error.message().to_string()))
    }
}

#[cfg(test)]
mod heap_insert_tests {
    use andromeda_storage_page::{PageId, PageSize};

    use super::*;

    #[test]
    fn test_insert_raw_single_tuple() {
        let mut insert =
            HeapPageInsert::new(PageId::new(1), PageSize::KiB16).expect("create insert context");

        let data = b"hello_world";
        let slot_id = insert.insert_raw_tuple(data).expect("insert");

        assert_eq!(slot_id, 0);
        let read = insert.read_tuple(slot_id).expect("read");
        assert_eq!(read, data);
    }

    #[test]
    fn test_insert_multiple_tuples() {
        let mut insert =
            HeapPageInsert::new(PageId::new(1), PageSize::KiB16).expect("create insert context");

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
        let mut insert =
            HeapPageInsert::new(PageId::new(1), PageSize::KiB16).expect("create insert context");

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
        let mut insert =
            HeapPageInsert::new(PageId::new(1), PageSize::KiB16).expect("create insert context");

        let result = insert.insert_raw_tuple(&[]);
        assert!(result.is_err());
        assert!(result.expect_err("should fail").message().contains("empty"));
    }

    #[test]
    fn test_insert_oversized_tuple_rejected() {
        let mut insert =
            HeapPageInsert::new(PageId::new(1), PageSize::KiB16).expect("create insert context");

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
    fn test_delete_tuple_logical() {
        let mut insert =
            HeapPageInsert::new(PageId::new(1), PageSize::KiB16).expect("create insert context");

        let slot_id = insert.insert_raw_tuple(b"test").expect("insert");
        assert_eq!(insert.active_slot_count(), 1);

        insert.delete_tuple(slot_id).expect("delete");
        assert_eq!(insert.active_slot_count(), 0);

        let result = insert.read_tuple(slot_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_page_metadata() {
        let insert =
            HeapPageInsert::new(PageId::new(42), PageSize::KiB16).expect("create insert context");

        assert_eq!(insert.page_id(), PageId::new(42));
        assert_eq!(insert.page_size(), PageSize::KiB16);
        assert_eq!(insert.slot_count(), 0);
        assert_eq!(insert.active_slot_count(), 0);
        assert!(insert.free_space() > 0);
    }

    #[test]
    fn test_multiple_small_insertions() {
        let mut insert =
            HeapPageInsert::new(PageId::new(1), PageSize::KiB16).expect("create insert context");

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
        let mut insert =
            HeapPageInsert::new(PageId::new(1), PageSize::KiB16).expect("create insert context");

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
    fn test_serialize_page() {
        let mut insert =
            HeapPageInsert::new(PageId::new(1), PageSize::KiB16).expect("create insert context");

        insert.insert_raw_tuple(b"data1").expect("insert 1");
        insert.insert_raw_tuple(b"data2").expect("insert 2");

        let serialized = insert.serialize().expect("serialize");
        assert_eq!(serialized.len(), PageSize::KiB16.bytes_usize());
    }

    #[test]
    fn test_free_space_tracking() {
        let mut insert =
            HeapPageInsert::new(PageId::new(1), PageSize::KiB16).expect("create insert context");

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
