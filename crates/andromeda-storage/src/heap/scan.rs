use andromeda_core::AndromedaResult;

use super::HeapPage;

pub struct HeapScanIter<'a> {
    page: &'a HeapPage,
    current_slot: u16,
}

impl HeapPage {
    pub fn scan(&self) -> HeapScanIter<'_> {
        HeapScanIter {
            page: self,
            current_slot: 0,
        }
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
