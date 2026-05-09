use andromeda_error::AndromedaResult;

use crate::error::storage_error;

pub(crate) fn write_u16(target: &mut [u8], offset: usize, value: u16) {
    target[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn write_u32(target: &mut [u8], offset: usize, value: u32) {
    target[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn write_u64(target: &mut [u8], offset: usize, value: u64) {
    target[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn read_u16(source: &[u8], offset: usize) -> AndromedaResult<u16> {
    Ok(u16::from_le_bytes(read_array_at(
        source,
        offset,
        "u16 field offset overflows",
        "truncated u16 field",
    )?))
}

pub(crate) fn read_u32(source: &[u8], offset: usize) -> AndromedaResult<u32> {
    Ok(u32::from_le_bytes(read_array_at(
        source,
        offset,
        "u32 field offset overflows",
        "truncated u32 field",
    )?))
}

pub(crate) fn read_u64(source: &[u8], offset: usize) -> AndromedaResult<u64> {
    Ok(u64::from_le_bytes(read_array_at(
        source,
        offset,
        "u64 field offset overflows",
        "truncated u64 field",
    )?))
}

fn read_array_at<const N: usize>(
    source: &[u8],
    offset: usize,
    overflow_msg: &'static str,
    truncated_msg: &'static str,
) -> AndromedaResult<[u8; N]> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| storage_error(overflow_msg))?;
    let mut bytes = [0u8; N];
    bytes.copy_from_slice(
        source
            .get(offset..end)
            .ok_or_else(|| storage_error(truncated_msg))?,
    );
    Ok(bytes)
}
