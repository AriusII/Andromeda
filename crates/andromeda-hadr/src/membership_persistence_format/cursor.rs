use andromeda_error::AndromedaResult;

use super::{read_u64, storage_error};

pub(super) struct PayloadCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> PayloadCursor<'a> {
    pub(super) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) fn read_u64(&mut self) -> AndromedaResult<u64> {
        let end = self
            .offset
            .checked_add(8)
            .ok_or_else(|| storage_error("HADR membership payload offset overflow"))?;
        if end > self.bytes.len() {
            return Err(storage_error("HADR membership payload is truncated"));
        }
        let value = read_u64(&self.bytes[self.offset..end])?;
        self.offset = end;
        Ok(value)
    }

    pub(super) fn read_u8(&mut self) -> AndromedaResult<u8> {
        if self.offset >= self.bytes.len() {
            return Err(storage_error("HADR membership payload is truncated"));
        }
        let value = self.bytes[self.offset];
        self.offset += 1;
        Ok(value)
    }

    pub(super) fn read_padding(&mut self, len: usize) -> AndromedaResult<()> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| storage_error("HADR membership payload offset overflow"))?;
        if end > self.bytes.len() {
            return Err(storage_error("HADR membership payload is truncated"));
        }
        if self.bytes[self.offset..end].iter().any(|byte| *byte != 0) {
            return Err(storage_error("HADR membership payload padding is invalid"));
        }
        self.offset = end;
        Ok(())
    }
}
