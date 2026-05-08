use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_storage_heap as heap_core;

use crate::heap_row_encoder::{Datum, ProductStockRow, RowEncoder, product_stock_row_encoder};
use crate::write_ahead_log::HeapRowRedoPayloadV1;
use crate::{Lsn, PageId, PageSize};

pub use heap_core::{
    HEAP_PAGE_V1_PAYLOAD_OFFSET, HeapVacuumMode, HeapVacuumPlan, HeapVacuumReport, SlotEntry,
    slot_directory,
};

#[derive(Debug, Clone)]
pub struct HeapPage {
    inner: heap_core::HeapPage,
}

impl HeapPage {
    pub fn new(page_size: PageSize) -> Self {
        Self {
            inner: heap_core::HeapPage::new(page_size),
        }
    }

    pub fn from_image(page_size: PageSize, bytes: &[u8]) -> AndromedaResult<Self> {
        heap_core::HeapPage::from_image(page_size, bytes).map(Self::from_inner)
    }

    pub fn from_blank_unallocated_image(
        page_size: PageSize,
        bytes: &[u8],
    ) -> AndromedaResult<Self> {
        heap_core::HeapPage::from_blank_unallocated_image(page_size, bytes).map(Self::from_inner)
    }

    pub fn page_size(&self) -> PageSize {
        self.inner.page_size()
    }

    pub fn slot_count(&self) -> usize {
        self.inner.slot_count()
    }

    pub fn live_row_count(&self) -> u32 {
        self.inner.live_row_count()
    }

    pub fn read_tuple(&self, slot_id: u16) -> AndromedaResult<Vec<u8>> {
        self.inner.read_tuple(slot_id)
    }

    pub fn serialize_slot_directory(&self) -> Vec<u8> {
        self.inner.serialize_slot_directory()
    }

    pub fn insert_tuple(&mut self, tuple: &[u8]) -> AndromedaResult<u16> {
        self.inner.insert_tuple(tuple)
    }

    pub fn update_tuple(&mut self, slot_id: u16, new_tuple: &[u8]) -> AndromedaResult<u16> {
        self.inner.update_tuple(slot_id, new_tuple)
    }

    pub fn delete_tuple(&mut self, slot_id: u16) -> AndromedaResult<()> {
        self.inner.delete_tuple(slot_id)
    }

    pub fn mark_deleted_batch(&mut self, slot_ids: &[u16]) -> AndromedaResult<usize> {
        self.inner.mark_deleted_batch(slot_ids)
    }

    pub fn has_deleted_slots(&self) -> bool {
        self.inner.has_deleted_slots()
    }

    pub fn compact_deleted(&mut self) -> AndromedaResult<u16> {
        self.inner.compact_deleted()
    }

    pub fn vacuum_plan(&self, mode: HeapVacuumMode) -> HeapVacuumPlan {
        self.inner.vacuum_plan(mode)
    }

    pub fn scan(&self) -> HeapScanIter<'_> {
        HeapScanIter {
            raw: self.inner.scan(),
        }
    }

    pub fn scan_product_stock(&self) -> ProductStockHeapScanIter<'_> {
        ProductStockHeapScanIter { raw: self.scan() }
    }

    fn from_inner(inner: heap_core::HeapPage) -> Self {
        Self { inner }
    }
}

pub struct HeapScanIter<'a> {
    raw: heap_core::HeapScanIter<'a>,
}

impl Iterator for HeapScanIter<'_> {
    type Item = AndromedaResult<(u16, Vec<u8>)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.raw.next()
    }
}

pub struct ProductStockHeapScanIter<'a> {
    raw: HeapScanIter<'a>,
}

impl Iterator for ProductStockHeapScanIter<'_> {
    type Item = AndromedaResult<(u16, ProductStockRow)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.raw.next().map(|item| {
            item.and_then(|(slot_id, tuple)| {
                ProductStockRow::decode(&tuple).map(|row| (slot_id, row))
            })
        })
    }
}

#[derive(Debug)]
pub struct HeapPageInsert {
    inner: heap_core::HeapPageInsert,
    row_encoder: Option<RowEncoder>,
}

impl HeapPageInsert {
    pub fn new(page_id: PageId, page_size: PageSize) -> AndromedaResult<Self> {
        Ok(Self {
            inner: heap_core::HeapPageInsert::new(page_id, page_size)?,
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
        self.inner.insert_raw_tuple(tuple_bytes)
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
            page_id: self.page_id(),
            page_size: self.page_size(),
            slot_id,
            row,
            tuple,
        })
    }

    pub fn read_product_stock(&self, slot_id: u16) -> AndromedaResult<ProductStockRow> {
        ProductStockRow::decode(&self.read_tuple(slot_id)?)
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
            self.page_id,
            self.page_size,
            self.slot_id,
            expected_previous_page_lsn,
            resulting_page_lsn,
            self.tuple.clone(),
        )
        .map_err(|error| heap_error(error.message().to_string()))
    }
}

fn heap_error(msg: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, msg)
}
