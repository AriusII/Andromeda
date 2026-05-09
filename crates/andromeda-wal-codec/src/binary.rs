use andromeda_error::AndromedaResult;

use crate::storage_error;

pub(crate) fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn read_u16(bytes: &[u8], offset: usize) -> AndromedaResult<u16> {
    Ok(u16::from_le_bytes(read_array(
        bytes,
        offset,
        "WAL frame u16 field out of bounds",
    )?))
}

pub(crate) fn read_u64(bytes: &[u8], offset: usize) -> AndromedaResult<u64> {
    Ok(u64::from_le_bytes(read_array(
        bytes,
        offset,
        "WAL frame u64 field out of bounds",
    )?))
}

fn read_array<const N: usize>(
    bytes: &[u8],
    offset: usize,
    error_message: &'static str,
) -> AndromedaResult<[u8; N]> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| storage_error(error_message))?;
    bytes
        .get(offset..end)
        .ok_or_else(|| storage_error(error_message))?
        .try_into()
        .map_err(|_| storage_error(error_message))
}
