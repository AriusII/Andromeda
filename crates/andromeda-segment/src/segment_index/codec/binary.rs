use super::super::error::{SegmentIndexError, SegmentIndexResult};

pub(super) fn write_u16(target: &mut [u8], offset: usize, value: u16) {
    target[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

pub(super) fn write_u32(target: &mut [u8], offset: usize, value: u32) {
    target[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

pub(super) fn write_u64(target: &mut [u8], offset: usize, value: u64) {
    target[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

pub(super) fn read_u16(
    source: &[u8],
    offset: usize,
    field: &'static str,
) -> SegmentIndexResult<u16> {
    let end = offset
        .checked_add(2)
        .ok_or(SegmentIndexError::LengthOverflow { field })?;
    let mut bytes = [0u8; 2];
    bytes.copy_from_slice(
        source
            .get(offset..end)
            .ok_or(SegmentIndexError::Truncated { field })?,
    );
    Ok(u16::from_le_bytes(bytes))
}

pub(super) fn read_u32(
    source: &[u8],
    offset: usize,
    field: &'static str,
) -> SegmentIndexResult<u32> {
    let end = offset
        .checked_add(4)
        .ok_or(SegmentIndexError::LengthOverflow { field })?;
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(
        source
            .get(offset..end)
            .ok_or(SegmentIndexError::Truncated { field })?,
    );
    Ok(u32::from_le_bytes(bytes))
}

pub(super) fn read_u64(
    source: &[u8],
    offset: usize,
    field: &'static str,
) -> SegmentIndexResult<u64> {
    let end = offset
        .checked_add(8)
        .ok_or(SegmentIndexError::LengthOverflow { field })?;
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(
        source
            .get(offset..end)
            .ok_or(SegmentIndexError::Truncated { field })?,
    );
    Ok(u64::from_le_bytes(bytes))
}

pub(super) fn read_hash(
    source: &[u8],
    offset: usize,
    field: &'static str,
) -> SegmentIndexResult<[u8; 32]> {
    let end = offset
        .checked_add(32)
        .ok_or(SegmentIndexError::LengthOverflow { field })?;
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(
        source
            .get(offset..end)
            .ok_or(SegmentIndexError::Truncated { field })?,
    );
    Ok(bytes)
}
