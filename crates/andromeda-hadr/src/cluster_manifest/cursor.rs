use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

pub(super) struct DecodeCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> DecodeCursor<'a> {
    pub(super) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) const fn consumed(&self) -> usize {
        self.offset
    }

    pub(super) fn read_u8(&mut self) -> AndromedaResult<u8> {
        Ok(self.read_exact(1)?[0])
    }

    pub(super) fn read_u16(&mut self) -> AndromedaResult<u16> {
        let array: [u8; 2] = self
            .read_exact(2)?
            .try_into()
            .map_err(|_| storage_error("cluster manifest u16 field is truncated"))?;
        Ok(u16::from_le_bytes(array))
    }

    pub(super) fn read_u64(&mut self) -> AndromedaResult<u64> {
        let array: [u8; 8] = self
            .read_exact(8)?
            .try_into()
            .map_err(|_| storage_error("cluster manifest u64 field is truncated"))?;
        Ok(u64::from_le_bytes(array))
    }

    pub(super) fn read_fixed<const N: usize>(&mut self) -> AndromedaResult<[u8; N]> {
        self.read_exact(N)?
            .try_into()
            .map_err(|_| storage_error("cluster manifest fixed field is truncated"))
    }

    pub(super) fn read_bytes(&mut self, len: usize) -> AndromedaResult<&'a [u8]> {
        self.read_exact(len)
    }

    fn read_exact(&mut self, len: usize) -> AndromedaResult<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| storage_error("cluster manifest decode cursor overflow"))?;
        if end > self.bytes.len() {
            return Err(storage_error("cluster manifest payload is truncated"));
        }
        let slice = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(slice)
    }
}

fn storage_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
