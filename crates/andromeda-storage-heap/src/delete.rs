use andromeda_error::AndromedaResult;

use super::{HeapPage, heap_error};

impl HeapPage {
    pub fn delete_tuple(&mut self, slot_id: u16) -> AndromedaResult<()> {
        let slot_id_usize = slot_id as usize;
        if slot_id_usize >= self.slot_directory.len() {
            return Err(heap_error(format!(
                "slot {} out of range: page has {} slots",
                slot_id,
                self.slot_directory.len()
            )));
        }

        if self.slot_directory[slot_id_usize].is_deleted() {
            return Err(heap_error(format!("slot {} already deleted", slot_id)));
        }

        self.slot_directory[slot_id_usize].mark_deleted();
        Ok(())
    }

    pub fn mark_deleted_batch(&mut self, slot_ids: &[u16]) -> AndromedaResult<usize> {
        if slot_ids.is_empty() {
            return Ok(0);
        }

        for &slot_id in slot_ids {
            let slot_id_usize = slot_id as usize;
            if slot_id_usize >= self.slot_directory.len() {
                return Err(heap_error(format!(
                    "slot {} out of range: page has {} slots",
                    slot_id,
                    self.slot_directory.len()
                )));
            }
            if self.slot_directory[slot_id_usize].is_deleted() {
                return Err(heap_error(format!("slot {} already deleted", slot_id)));
            }
        }

        for &slot_id in slot_ids {
            let slot_id_usize = slot_id as usize;
            self.slot_directory[slot_id_usize].mark_deleted();
        }

        Ok(slot_ids.len())
    }

    pub fn has_deleted_slots(&self) -> bool {
        self.slot_directory.iter().any(|e| e.is_deleted())
    }
}
