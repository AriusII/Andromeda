use andromeda_core::AndromedaResult;

use crate::heap_row_encoder::ProductStockRow;

use super::HeapPage;

pub struct HeapScanIter<'a> {
    page: &'a HeapPage,
    current_slot: u16,
}

pub struct ProductStockHeapScanIter<'a> {
    raw: HeapScanIter<'a>,
}

impl HeapPage {
    pub fn scan(&self) -> HeapScanIter<'_> {
        HeapScanIter {
            page: self,
            current_slot: 0,
        }
    }

    pub fn scan_product_stock(&self) -> ProductStockHeapScanIter<'_> {
        ProductStockHeapScanIter { raw: self.scan() }
    }
}

impl<'a> Iterator for HeapScanIter<'a> {
    type Item = AndromedaResult<(u16, Vec<u8>)>;

    fn next(&mut self) -> Option<Self::Item> {
        while (self.current_slot as usize) < self.page.slot_directory.len() {
            let slot_id = self.current_slot;
            self.current_slot += 1;

            match self.page.read_tuple(slot_id) {
                Ok(data) => return Some(Ok((slot_id, data))),
                Err(e) if e.message().contains("deleted") => continue,
                Err(e) => return Some(Err(e)),
            }
        }
        None
    }
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
