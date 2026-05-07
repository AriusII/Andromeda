use andromeda_core::AndromedaResult;

use crate::PageId;

use super::{
    header::BTREE_NODE_V1_HEADER_LEN,
    validation::{btree_node_format_error, checked_u16},
};

pub(super) fn leaf_body_len(keys: &[Vec<u8>], values: &[Vec<u8>]) -> AndromedaResult<usize> {
    if keys.len() != values.len() {
        return Err(btree_node_format_error(
            "BTree leaf keys and values must have identical cardinality",
        ));
    }
    keys.iter()
        .zip(values)
        .try_fold(0usize, |acc, (key, value)| {
            let with_key = acc
                .checked_add(2)
                .and_then(|v| v.checked_add(key.len()))
                .ok_or_else(|| btree_node_format_error("BTree leaf key area length overflow"))?;
            with_key
                .checked_add(2)
                .and_then(|v| v.checked_add(value.len()))
                .ok_or_else(|| btree_node_format_error("BTree leaf value area length overflow"))
        })
}

pub(super) fn internal_body_len(keys: &[Vec<u8>], children: &[PageId]) -> AndromedaResult<usize> {
    let child_bytes = children
        .len()
        .checked_mul(8)
        .ok_or_else(|| btree_node_format_error("BTree child pointer area length overflow"))?;
    keys.iter().try_fold(child_bytes, |acc, key| {
        acc.checked_add(2)
            .and_then(|v| v.checked_add(key.len()))
            .ok_or_else(|| btree_node_format_error("BTree internal key area length overflow"))
    })
}

pub(super) fn checked_free_start(body_len: usize) -> AndromedaResult<u16> {
    checked_u16(
        BTREE_NODE_V1_HEADER_LEN
            .checked_add(body_len)
            .ok_or_else(|| btree_node_format_error("BTree node body length overflow"))?,
        "BTree node free_start exceeds u16",
    )
}

pub(super) fn validate_leaf_body_min_len(key_count: u16, body_len: usize) -> AndromedaResult<()> {
    let min_len = usize::from(key_count)
        .checked_mul(4)
        .ok_or_else(|| btree_node_format_error("BTree leaf body minimum length overflow"))?;
    if min_len > body_len {
        return Err(btree_node_format_error(
            "BTree leaf key_count exceeds encoded body length",
        ));
    }
    Ok(())
}

pub(super) fn validate_internal_body_min_len(
    key_count: u16,
    child_count: u16,
    body_len: usize,
) -> AndromedaResult<()> {
    let child_bytes = usize::from(child_count)
        .checked_mul(8)
        .ok_or_else(|| btree_node_format_error("BTree internal child area length overflow"))?;
    let key_prefix_bytes = usize::from(key_count)
        .checked_mul(2)
        .ok_or_else(|| btree_node_format_error("BTree internal key prefix length overflow"))?;
    let min_len = child_bytes
        .checked_add(key_prefix_bytes)
        .ok_or_else(|| btree_node_format_error("BTree internal body minimum length overflow"))?;
    if min_len > body_len {
        return Err(btree_node_format_error(
            "BTree internal child/key counts exceed encoded body length",
        ));
    }
    Ok(())
}

pub(super) fn high_key_offset_for_leaf(keys: &[Vec<u8>]) -> Option<u16> {
    if keys.is_empty() {
        return None;
    }
    let mut offset = BTREE_NODE_V1_HEADER_LEN;
    for key in &keys[..keys.len() - 1] {
        offset += 2 + key.len();
    }
    u16::try_from(offset).ok()
}

pub(super) fn high_key_offset_for_internal(child_count: usize, keys: &[Vec<u8>]) -> Option<u16> {
    if keys.is_empty() {
        return None;
    }
    let mut offset = BTREE_NODE_V1_HEADER_LEN + child_count * 8;
    for key in &keys[..keys.len() - 1] {
        offset += 2 + key.len();
    }
    u16::try_from(offset).ok()
}

pub(super) fn write_len_prefixed_bytes(
    target: &mut [u8],
    offset: &mut usize,
    bytes: &[u8],
) -> AndromedaResult<()> {
    let len = checked_u16(bytes.len(), "BTree node byte field exceeds u16")?;
    write_u16_at(target, offset, len)?;
    let end = offset
        .checked_add(bytes.len())
        .ok_or_else(|| btree_node_format_error("BTree node write offset overflow"))?;
    target
        .get_mut(*offset..end)
        .ok_or_else(|| btree_node_format_error("BTree node encoded body exceeds page image"))?
        .copy_from_slice(bytes);
    *offset = end;
    Ok(())
}

pub(super) fn read_len_prefixed_bytes(
    source: &[u8],
    offset: &mut usize,
    limit: usize,
) -> AndromedaResult<Vec<u8>> {
    let len = read_u16_at(source, offset, limit)? as usize;
    let end = (*offset)
        .checked_add(len)
        .ok_or_else(|| btree_node_format_error("BTree node read offset overflow"))?;
    if end > limit {
        return Err(btree_node_format_error(
            "BTree node length-prefixed field exceeds free_start",
        ));
    }
    let bytes = source
        .get(*offset..end)
        .ok_or_else(|| btree_node_format_error("BTree node length-prefixed field truncated"))?
        .to_vec();
    *offset = end;
    Ok(bytes)
}

fn write_u16_at(target: &mut [u8], offset: &mut usize, value: u16) -> AndromedaResult<()> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| btree_node_format_error("BTree node write offset overflow"))?;
    target
        .get_mut(*offset..end)
        .ok_or_else(|| btree_node_format_error("BTree node u16 write exceeds page image"))?
        .copy_from_slice(&value.to_le_bytes());
    *offset = end;
    Ok(())
}

pub(super) fn write_u64_at(
    target: &mut [u8],
    offset: &mut usize,
    value: u64,
) -> AndromedaResult<()> {
    let end = offset
        .checked_add(8)
        .ok_or_else(|| btree_node_format_error("BTree node write offset overflow"))?;
    target
        .get_mut(*offset..end)
        .ok_or_else(|| btree_node_format_error("BTree node u64 write exceeds page image"))?
        .copy_from_slice(&value.to_le_bytes());
    *offset = end;
    Ok(())
}

fn read_u16_at(source: &[u8], offset: &mut usize, limit: usize) -> AndromedaResult<u16> {
    let end = (*offset)
        .checked_add(2)
        .ok_or_else(|| btree_node_format_error("BTree node read offset overflow"))?;
    if end > limit {
        return Err(btree_node_format_error(
            "BTree node u16 field exceeds free_start",
        ));
    }
    let value = read_u16(source, *offset)?;
    *offset = end;
    Ok(value)
}

pub(super) fn read_u64_at(source: &[u8], offset: &mut usize, limit: usize) -> AndromedaResult<u64> {
    let end = (*offset)
        .checked_add(8)
        .ok_or_else(|| btree_node_format_error("BTree node read offset overflow"))?;
    if end > limit {
        return Err(btree_node_format_error(
            "BTree node u64 field exceeds free_start",
        ));
    }
    let value = read_u64(source, *offset)?;
    *offset = end;
    Ok(value)
}

fn read_u16(source: &[u8], offset: usize) -> AndromedaResult<u16> {
    let mut bytes = [0u8; 2];
    bytes.copy_from_slice(
        source
            .get(offset..offset + 2)
            .ok_or_else(|| btree_node_format_error("BTree node u16 field is truncated"))?,
    );
    Ok(u16::from_le_bytes(bytes))
}

fn read_u64(source: &[u8], offset: usize) -> AndromedaResult<u64> {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(
        source
            .get(offset..offset + 8)
            .ok_or_else(|| btree_node_format_error("BTree node u64 field is truncated"))?,
    );
    Ok(u64::from_le_bytes(bytes))
}
