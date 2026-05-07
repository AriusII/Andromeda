//! Primitive field writers and the checked decode cursor.

use andromeda_error::AndromedaResult;

use super::catalog_error;

pub(super) fn push_bool(out: &mut Vec<u8>, value: bool) {
    push_u8(out, u8::from(value));
}

pub(super) fn push_u8(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}

pub(super) fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn push_i64(out: &mut Vec<u8>, value: i64) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(super) struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) fn finish(&self) -> AndromedaResult<()> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(catalog_error("trailing bytes in catalog WAL payload body"))
        }
    }

    pub(super) fn bytes(&mut self, len: usize) -> AndromedaResult<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| catalog_error("catalog WAL payload offset overflow"))?;
        if end > self.bytes.len() {
            return Err(catalog_error("truncated catalog WAL payload body"));
        }
        let slice = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(slice)
    }

    pub(super) fn u8(&mut self) -> AndromedaResult<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub(super) fn bool(&mut self) -> AndromedaResult<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(catalog_error("unknown bool tag")),
        }
    }

    pub(super) fn u16(&mut self) -> AndromedaResult<u16> {
        let bytes = self
            .bytes(2)?
            .try_into()
            .map_err(|_| catalog_error("catalog WAL u16 field has invalid width"))?;
        Ok(u16::from_le_bytes(bytes))
    }

    pub(super) fn u32(&mut self) -> AndromedaResult<u32> {
        let bytes = self
            .bytes(4)?
            .try_into()
            .map_err(|_| catalog_error("catalog WAL u32 field has invalid width"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    pub(super) fn u64(&mut self) -> AndromedaResult<u64> {
        let bytes = self
            .bytes(8)?
            .try_into()
            .map_err(|_| catalog_error("catalog WAL u64 field has invalid width"))?;
        Ok(u64::from_le_bytes(bytes))
    }

    pub(super) fn i64(&mut self) -> AndromedaResult<i64> {
        let bytes = self
            .bytes(8)?
            .try_into()
            .map_err(|_| catalog_error("catalog WAL i64 field has invalid width"))?;
        Ok(i64::from_le_bytes(bytes))
    }

    pub(super) fn len(&mut self) -> AndromedaResult<usize> {
        usize::try_from(self.u64()?)
            .map_err(|_| catalog_error("catalog WAL collection length does not fit usize"))
    }

    pub(super) fn string(&mut self) -> AndromedaResult<String> {
        let len = self.len()?;
        let bytes = self.bytes(len)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|_| catalog_error("catalog WAL string is not valid UTF-8"))
    }
}
