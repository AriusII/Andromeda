use andromeda_core::AndromedaResult;

use crate::Lsn;

use super::super::{
    artifacts::BackupArtifactDigest,
    helpers::{backup_error, map_backup_validation},
};

pub(super) fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn push_lsn(bytes: &mut Vec<u8>, lsn: Lsn) {
    push_u64(bytes, lsn.get());
}

pub(super) fn push_option_lsn(bytes: &mut Vec<u8>, lsn: Option<Lsn>) {
    match lsn {
        Some(value) => {
            bytes.push(1);
            push_lsn(bytes, value);
        },
        None => {
            bytes.push(0);
            push_u64(bytes, 0);
        },
    }
}

pub(super) fn push_digest(bytes: &mut Vec<u8>, digest: BackupArtifactDigest) {
    bytes.extend_from_slice(&digest.sha256);
    push_u64(bytes, digest.crc64);
    push_u64(bytes, digest.byte_len);
}

pub(super) fn read_u16_at(bytes: &[u8], offset: usize) -> AndromedaResult<u16> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| backup_error("backup artifact manifest offset overflow"))?;
    let array: [u8; 2] = bytes
        .get(offset..end)
        .ok_or_else(|| backup_error("backup artifact manifest u16 field is truncated"))?
        .try_into()
        .map_err(|_| backup_error("backup artifact manifest u16 field is truncated"))?;
    Ok(u16::from_le_bytes(array))
}

pub(super) fn read_u64_at(bytes: &[u8], offset: usize) -> AndromedaResult<u64> {
    let end = offset
        .checked_add(8)
        .ok_or_else(|| backup_error("backup artifact manifest offset overflow"))?;
    let array: [u8; 8] = bytes
        .get(offset..end)
        .ok_or_else(|| backup_error("backup artifact manifest u64 field is truncated"))?
        .try_into()
        .map_err(|_| backup_error("backup artifact manifest u64 field is truncated"))?;
    Ok(u64::from_le_bytes(array))
}

pub(super) struct PayloadCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> PayloadCursor<'a> {
    pub(super) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) fn finish(&self) -> AndromedaResult<()> {
        if self.offset != self.bytes.len() {
            return Err(backup_error(
                "backup artifact manifest payload has trailing bytes",
            ));
        }
        Ok(())
    }

    fn read_exact(&mut self, len: usize) -> AndromedaResult<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| backup_error("backup artifact manifest payload offset overflow"))?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| backup_error("backup artifact manifest payload is truncated"))?;
        self.offset = end;
        Ok(slice)
    }

    pub(super) fn read_u64(&mut self) -> AndromedaResult<u64> {
        let array: [u8; 8] = self
            .read_exact(8)?
            .try_into()
            .map_err(|_| backup_error("backup artifact manifest u64 field is truncated"))?;
        Ok(u64::from_le_bytes(array))
    }

    pub(super) fn read_u32(&mut self) -> AndromedaResult<u32> {
        let array: [u8; 4] = self
            .read_exact(4)?
            .try_into()
            .map_err(|_| backup_error("backup artifact manifest u32 field is truncated"))?;
        Ok(u32::from_le_bytes(array))
    }

    pub(super) fn read_u16(&mut self) -> AndromedaResult<u16> {
        let array: [u8; 2] = self
            .read_exact(2)?
            .try_into()
            .map_err(|_| backup_error("backup artifact manifest u16 field is truncated"))?;
        Ok(u16::from_le_bytes(array))
    }

    fn read_u8(&mut self) -> AndromedaResult<u8> {
        Ok(*self
            .read_exact(1)?
            .first()
            .ok_or_else(|| backup_error("backup artifact manifest u8 field is truncated"))?)
    }

    pub(super) fn read_lsn(&mut self) -> AndromedaResult<Lsn> {
        Ok(Lsn::new(self.read_u64()?))
    }

    pub(super) fn read_option_lsn(&mut self) -> AndromedaResult<Option<Lsn>> {
        let tag = self.read_u8()?;
        let value = Lsn::new(self.read_u64()?);
        match tag {
            0 => {
                if !value.is_zero() {
                    return Err(backup_error(
                        "backup artifact manifest absent LSN must encode zero value",
                    ));
                }
                Ok(None)
            },
            1 => {
                if value.is_zero() {
                    return Err(backup_error(
                        "backup artifact manifest present LSN must not be zero",
                    ));
                }
                Ok(Some(value))
            },
            _ => Err(backup_error(
                "backup artifact manifest optional LSN tag is invalid",
            )),
        }
    }

    pub(super) fn read_array_32(&mut self) -> AndromedaResult<[u8; 32]> {
        self.read_exact(32)?
            .try_into()
            .map_err(|_| backup_error("backup artifact manifest 32-byte field is truncated"))
    }

    pub(super) fn read_digest(&mut self) -> AndromedaResult<BackupArtifactDigest> {
        let digest = BackupArtifactDigest {
            sha256: self.read_array_32()?,
            crc64: self.read_u64()?,
            byte_len: self.read_u64()?,
        };
        map_backup_validation(digest.validate("backup artifact manifest digest"))?;
        Ok(digest)
    }
}
