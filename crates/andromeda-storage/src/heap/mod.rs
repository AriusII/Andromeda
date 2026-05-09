use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_storage_heap as heap_core;

use crate::write_ahead_log::HeapRowRedoPayloadV1;
use crate::{Datum, Lsn, PageId, PageSize, ProductStockRow, RowEncoder};

pub use heap_core::{
    HEAP_PAGE_V1_PAYLOAD_OFFSET, HeapPage, HeapScanIter, HeapVacuumMode, HeapVacuumPlan,
    HeapVacuumReport, ProductStockHeapScanIter, SlotEntry, slot_directory,
};

#[derive(Debug)]
pub struct HeapPageInsert {
    inner: heap_core::HeapPageInsert,
}

impl HeapPageInsert {
    pub fn new(page_id: PageId, page_size: PageSize) -> AndromedaResult<Self> {
        Ok(Self {
            inner: heap_core::HeapPageInsert::new(page_id, page_size)?,
        })
    }

    pub fn for_product_stock(page_id: PageId, page_size: PageSize) -> AndromedaResult<Self> {
        Ok(Self {
            inner: heap_core::HeapPageInsert::for_product_stock(page_id, page_size)?,
        })
    }

    pub fn with_encoder(mut self, encoder: RowEncoder) -> Self {
        self.inner = self.inner.with_encoder(encoder);
        self
    }

    pub fn insert_raw_tuple(&mut self, tuple_bytes: &[u8]) -> AndromedaResult<u16> {
        self.inner.insert_raw_tuple(tuple_bytes)
    }

    pub fn insert_tuple(&mut self, datums: &[Datum]) -> AndromedaResult<u16> {
        self.inner.insert_tuple(datums)
    }

    pub fn insert_product_stock(
        &mut self,
        row: ProductStockRow,
    ) -> AndromedaResult<ProductStockHeapInsert> {
        self.inner
            .insert_product_stock(row)
            .map(ProductStockHeapInsert::from_inner)
    }

    pub fn read_product_stock(&self, slot_id: u16) -> AndromedaResult<ProductStockRow> {
        self.inner.read_product_stock(slot_id)
    }

    pub fn read_tuple(&self, slot_id: u16) -> AndromedaResult<Vec<u8>> {
        self.inner.read_tuple(slot_id)
    }

    pub fn delete_tuple(&mut self, slot_id: u16) -> AndromedaResult<()> {
        self.inner.delete_tuple(slot_id)
    }

    pub fn page_id(&self) -> PageId {
        self.inner.page_id()
    }

    pub fn page_size(&self) -> PageSize {
        self.inner.page_size()
    }

    pub fn slot_count(&self) -> usize {
        self.inner.slot_count()
    }

    pub fn active_slot_count(&self) -> usize {
        self.inner.active_slot_count()
    }

    pub fn free_space(&self) -> u16 {
        self.inner.free_space()
    }

    pub fn serialize(&mut self) -> AndromedaResult<Vec<u8>> {
        self.inner.serialize()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductStockHeapInsert {
    inner: heap_core::ProductStockHeapInsert,
}

impl ProductStockHeapInsert {
    fn from_inner(inner: heap_core::ProductStockHeapInsert) -> Self {
        Self { inner }
    }

    pub const fn page_id(&self) -> PageId {
        self.inner.page_id()
    }

    pub const fn page_size(&self) -> PageSize {
        self.inner.page_size()
    }

    pub const fn slot_id(&self) -> u16 {
        self.inner.slot_id()
    }

    pub const fn row(&self) -> ProductStockRow {
        self.inner.row()
    }

    pub fn tuple(&self) -> &[u8] {
        self.inner.tuple()
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

fn heap_error(msg: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, msg)
}
