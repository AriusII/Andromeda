use andromeda_error::AndromedaResult;

use super::{SlotEntry, heap_error};

pub(crate) fn checked_tuple_len(
    tuple: &[u8],
    empty_msg: Option<&'static str>,
    too_large_msg: impl FnOnce(usize) -> String,
) -> AndromedaResult<u16> {
    if tuple.is_empty() {
        if let Some(message) = empty_msg {
            return Err(heap_error(message));
        }
    }
    if tuple.len() > u16::MAX as usize {
        return Err(heap_error(too_large_msg(tuple.len())));
    }
    Ok(tuple.len() as u16)
}

pub(crate) fn tuple_insert_space(tuple_len: u16) -> AndromedaResult<usize> {
    usize::from(tuple_len)
        .checked_add(SlotEntry::SIZE)
        .ok_or_else(|| heap_error("tuple allocation space overflow"))
}

pub(crate) fn write_tuple_bytes(
    data: &mut [u8],
    offset: u16,
    tuple: &[u8],
    bounds_msg: &'static str,
) -> AndromedaResult<()> {
    let offset = usize::from(offset);
    let end = checked_tuple_end(offset, tuple.len(), data.len(), bounds_msg, bounds_msg)?;
    data[offset..end].copy_from_slice(tuple);
    Ok(())
}

pub(crate) fn read_tuple_bytes(
    data: &[u8],
    offset: u16,
    length: u16,
    overflow_msg: impl Into<String>,
    bounds_msg: impl Into<String>,
) -> AndromedaResult<Vec<u8>> {
    let offset = usize::from(offset);
    let length = usize::from(length);
    let end = checked_tuple_end(offset, length, data.len(), overflow_msg, bounds_msg)?;
    Ok(data[offset..end].to_vec())
}

fn checked_tuple_end(
    offset: usize,
    length: usize,
    data_len: usize,
    overflow_msg: impl Into<String>,
    bounds_msg: impl Into<String>,
) -> AndromedaResult<usize> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| heap_error(overflow_msg))?;
    if end > data_len {
        return Err(heap_error(bounds_msg));
    }
    Ok(end)
}
