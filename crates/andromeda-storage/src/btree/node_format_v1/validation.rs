use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::header::{
    BTREE_NODE_V1_FORMAT_VERSION, BTREE_NODE_V1_HEADER_LEN_U16, BTREE_NODE_V1_MAGIC,
    BTreeNodeHeaderV1, BTreeNodeKindV1, encode_header_without_validation, header_crc32,
};

pub(super) fn validate_header(header: &BTreeNodeHeaderV1) -> AndromedaResult<()> {
    if header.magic != BTREE_NODE_V1_MAGIC {
        return Err(btree_node_format_error("BTree node V1 magic mismatch"));
    }
    if header.format_version != BTREE_NODE_V1_FORMAT_VERSION {
        return Err(btree_node_format_error(
            "unsupported BTree node V1 format version",
        ));
    }
    if header.page_id.is_zero() {
        return Err(btree_node_format_error(
            "BTree node page_id must not be zero",
        ));
    }
    if header.page_lsn.is_zero() {
        return Err(btree_node_format_error(
            "BTree node page_lsn must not be zero",
        ));
    }
    if header.free_start < BTREE_NODE_V1_HEADER_LEN_U16 {
        return Err(btree_node_format_error(
            "BTree node free_start overlaps fixed header",
        ));
    }
    if header.free_start > header.free_end {
        return Err(btree_node_format_error(
            "BTree node free_start must be <= free_end",
        ));
    }
    if matches!(header.prev_leaf, Some(prev) if prev == header.page_id)
        || matches!(header.next_leaf, Some(next) if next == header.page_id)
    {
        return Err(btree_node_format_error(
            "BTree leaf links must not point to self",
        ));
    }
    if matches!((header.prev_leaf, header.next_leaf), (Some(prev), Some(next)) if prev == next) {
        return Err(btree_node_format_error(
            "BTree previous and next leaf links must differ",
        ));
    }
    match header.node_kind {
        BTreeNodeKindV1::Leaf => {
            if header.child_count != 0 {
                return Err(btree_node_format_error(
                    "BTree leaf node must not advertise child pointers",
                ));
            }
        }
        BTreeNodeKindV1::Internal => {
            let expected = header
                .key_count
                .checked_add(1)
                .ok_or_else(|| btree_node_format_error("BTree internal child count overflow"))?;
            if header.child_count != expected {
                return Err(btree_node_format_error(
                    "BTree internal child_count must equal key_count + 1",
                ));
            }
            if header.prev_leaf.is_some() || header.next_leaf.is_some() {
                return Err(btree_node_format_error(
                    "BTree internal nodes must not carry leaf sibling links",
                ));
            }
        }
    }
    if let Some(high_key_offset) = header.high_key_offset
        && (high_key_offset < BTREE_NODE_V1_HEADER_LEN_U16 || high_key_offset >= header.free_start)
    {
        return Err(btree_node_format_error(
            "BTree high_key_offset must point inside encoded key area",
        ));
    }

    let mut expected_header = header.clone();
    expected_header.header_crc = 0;
    let expected = header_crc32(&encode_header_without_validation(&expected_header));
    let expected = if expected == 0 { 1 } else { expected };
    if header.header_crc != expected {
        return Err(btree_node_format_error("BTree node header CRC mismatch"));
    }
    Ok(())
}

pub(super) fn validate_key_order(keys: &[Vec<u8>]) -> AndromedaResult<()> {
    for pair in keys.windows(2) {
        if pair[0] >= pair[1] {
            return Err(btree_node_format_error(
                "BTree node keys must be strictly ordered",
            ));
        }
    }
    Ok(())
}

pub(super) fn checked_u16(value: usize, message: &'static str) -> AndromedaResult<u16> {
    u16::try_from(value).map_err(|_| btree_node_format_error(message))
}

pub(super) fn btree_node_format_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
