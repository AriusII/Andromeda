use andromeda_core::AndromedaResult;

use crate::{Lsn, PageId};

mod body;
mod header;
mod validation;

use body::{
    checked_free_start, high_key_offset_for_internal, high_key_offset_for_leaf, internal_body_len,
    leaf_body_len, read_len_prefixed_bytes, read_u64_at, validate_internal_body_min_len,
    validate_leaf_body_min_len, write_len_prefixed_bytes, write_u64_at,
};
use header::{decode_header, encode_header_without_validation};
use validation::{btree_node_format_error, checked_u16, validate_key_order};

pub use header::{
    BTREE_NODE_V1_FORMAT_VERSION, BTREE_NODE_V1_HEADER_LEN, BTREE_NODE_V1_MAGIC, BTreeNodeHeaderV1,
    BTreeNodeKindV1,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BTreeNodeV1 {
    pub header: BTreeNodeHeaderV1,
    pub keys: Vec<Vec<u8>>,
    pub leaf_values: Vec<Vec<u8>>,
    pub child_page_ids: Vec<PageId>,
    page_size: u16,
}

impl BTreeNodeV1 {
    pub fn new_leaf(
        page_id: PageId,
        page_lsn: Lsn,
        entries: Vec<(Vec<u8>, Vec<u8>)>,
        prev_leaf: Option<PageId>,
        next_leaf: Option<PageId>,
        page_size: u16,
    ) -> AndromedaResult<Self> {
        let (keys, leaf_values): (Vec<_>, Vec<_>) = entries.into_iter().unzip();
        validate_key_order(&keys)?;
        let body_len = leaf_body_len(&keys, &leaf_values)?;
        let free_start = checked_free_start(body_len)?;
        if free_start > page_size {
            return Err(btree_node_format_error(
                "BTree leaf node body exceeds configured page size",
            ));
        }
        let high_key_offset = high_key_offset_for_leaf(&keys);
        let header = BTreeNodeHeaderV1::new(
            BTreeNodeKindV1::Leaf,
            page_id,
            page_lsn,
            checked_u16(keys.len(), "BTree leaf key_count exceeds u16")?,
            0,
            free_start,
            page_size,
        )
        .with_leaf_links(prev_leaf, next_leaf)
        .with_high_key_offset(high_key_offset)
        .with_computed_crc()?;
        let node = Self {
            header,
            keys,
            leaf_values,
            child_page_ids: Vec::new(),
            page_size,
        };
        node.validate()?;
        Ok(node)
    }

    pub fn new_internal(
        page_id: PageId,
        page_lsn: Lsn,
        keys: Vec<Vec<u8>>,
        child_page_ids: Vec<PageId>,
        page_size: u16,
    ) -> AndromedaResult<Self> {
        validate_key_order(&keys)?;
        if child_page_ids.len() != keys.len() + 1 {
            return Err(btree_node_format_error(
                "BTree internal child_page_ids must equal keys + 1",
            ));
        }
        if child_page_ids.iter().any(|page_id| page_id.is_zero()) {
            return Err(btree_node_format_error(
                "BTree internal child page ids must not be zero",
            ));
        }
        let body_len = internal_body_len(&keys, &child_page_ids)?;
        let free_start = checked_free_start(body_len)?;
        if free_start > page_size {
            return Err(btree_node_format_error(
                "BTree internal node body exceeds configured page size",
            ));
        }
        let high_key_offset = high_key_offset_for_internal(child_page_ids.len(), &keys);
        let header = BTreeNodeHeaderV1::new(
            BTreeNodeKindV1::Internal,
            page_id,
            page_lsn,
            checked_u16(keys.len(), "BTree internal key_count exceeds u16")?,
            checked_u16(
                child_page_ids.len(),
                "BTree internal child_count exceeds u16",
            )?,
            free_start,
            page_size,
        )
        .with_high_key_offset(high_key_offset)
        .with_computed_crc()?;
        let node = Self {
            header,
            keys,
            leaf_values: Vec::new(),
            child_page_ids,
            page_size,
        };
        node.validate()?;
        Ok(node)
    }

    pub const fn page_size(&self) -> u16 {
        self.page_size
    }

    pub const fn key_count(&self) -> usize {
        self.header.key_count as usize
    }

    pub const fn encoded_len(&self) -> usize {
        self.header.free_start as usize
    }

    pub fn encode(&self) -> AndromedaResult<Vec<u8>> {
        self.validate()?;
        let mut page = vec![0u8; self.page_size as usize];
        page[..BTREE_NODE_V1_HEADER_LEN].copy_from_slice(&self.encode_header()?);
        let mut offset = BTREE_NODE_V1_HEADER_LEN;
        match self.header.node_kind {
            BTreeNodeKindV1::Leaf => {
                for (key, value) in self.keys.iter().zip(&self.leaf_values) {
                    write_len_prefixed_bytes(&mut page, &mut offset, key)?;
                    write_len_prefixed_bytes(&mut page, &mut offset, value)?;
                }
            }
            BTreeNodeKindV1::Internal => {
                for child in &self.child_page_ids {
                    write_u64_at(&mut page, &mut offset, child.get())?;
                }
                for key in &self.keys {
                    write_len_prefixed_bytes(&mut page, &mut offset, key)?;
                }
            }
        }
        debug_assert_eq!(offset, self.header.free_start as usize);
        Ok(page)
    }

    pub fn decode(bytes: &[u8]) -> AndromedaResult<Self> {
        if bytes.len() < BTREE_NODE_V1_HEADER_LEN {
            return Err(btree_node_format_error("BTree node image is truncated"));
        }
        let header = decode_header(&bytes[..BTREE_NODE_V1_HEADER_LEN])?;
        if bytes.len() != header.free_end as usize {
            return Err(btree_node_format_error(
                "BTree node image length must match header free_end/page size",
            ));
        }
        let page_size = header.free_end;
        let body_limit = header.free_start as usize;
        let body_len = body_limit
            .checked_sub(BTREE_NODE_V1_HEADER_LEN)
            .ok_or_else(|| btree_node_format_error("BTree node body length underflows header"))?;
        let mut offset = BTREE_NODE_V1_HEADER_LEN;
        match header.node_kind {
            BTreeNodeKindV1::Leaf => {
                validate_leaf_body_min_len(header.key_count, body_len)?;
                let mut keys = Vec::with_capacity(header.key_count as usize);
                let mut values = Vec::with_capacity(header.key_count as usize);
                for _ in 0..header.key_count {
                    keys.push(read_len_prefixed_bytes(bytes, &mut offset, body_limit)?);
                    values.push(read_len_prefixed_bytes(bytes, &mut offset, body_limit)?);
                }
                if offset != header.free_start as usize {
                    return Err(btree_node_format_error(
                        "BTree leaf node encoded body does not match free_start",
                    ));
                }
                let node = Self {
                    header,
                    keys,
                    leaf_values: values,
                    child_page_ids: Vec::new(),
                    page_size,
                };
                node.validate()?;
                Ok(node)
            }
            BTreeNodeKindV1::Internal => {
                validate_internal_body_min_len(header.key_count, header.child_count, body_len)?;
                let mut children = Vec::with_capacity(header.child_count as usize);
                for _ in 0..header.child_count {
                    children.push(PageId::new(read_u64_at(bytes, &mut offset, body_limit)?));
                }
                let mut keys = Vec::with_capacity(header.key_count as usize);
                for _ in 0..header.key_count {
                    keys.push(read_len_prefixed_bytes(bytes, &mut offset, body_limit)?);
                }
                if offset != header.free_start as usize {
                    return Err(btree_node_format_error(
                        "BTree internal node encoded body does not match free_start",
                    ));
                }
                let node = Self {
                    header,
                    keys,
                    leaf_values: Vec::new(),
                    child_page_ids: children,
                    page_size,
                };
                node.validate()?;
                Ok(node)
            }
        }
    }

    /// Decode a node image read from an expected physical page.
    ///
    /// The page store owns physical addressability. The B-Tree node image must
    /// carry the same `page_id`, or recovery could replay a valid image into
    /// the wrong page.
    pub fn decode_for_page_id(bytes: &[u8], expected_page_id: PageId) -> AndromedaResult<Self> {
        if expected_page_id.is_zero() {
            return Err(btree_node_format_error(
                "BTree expected page_id must not be zero",
            ));
        }
        let node = Self::decode(bytes)?;
        if node.header.page_id != expected_page_id {
            return Err(btree_node_format_error(
                "BTree node page_id does not match expected page id",
            ));
        }
        Ok(node)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.header.validate()?;
        if self.page_size != self.header.free_end {
            return Err(btree_node_format_error(
                "BTree node page_size must match header free_end",
            ));
        }
        validate_key_order(&self.keys)?;
        match self.header.node_kind {
            BTreeNodeKindV1::Leaf => {
                if self.header.key_count as usize != self.keys.len()
                    || self.leaf_values.len() != self.keys.len()
                    || !self.child_page_ids.is_empty()
                {
                    return Err(btree_node_format_error(
                        "BTree leaf node key/value/child counts are inconsistent",
                    ));
                }
                if checked_free_start(leaf_body_len(&self.keys, &self.leaf_values)?)?
                    != self.header.free_start
                {
                    return Err(btree_node_format_error(
                        "BTree leaf node free_start does not match encoded body",
                    ));
                }
                if self.header.high_key_offset != high_key_offset_for_leaf(&self.keys) {
                    return Err(btree_node_format_error(
                        "BTree leaf high_key_offset does not match encoded keys",
                    ));
                }
            }
            BTreeNodeKindV1::Internal => {
                if self.header.key_count as usize != self.keys.len()
                    || self.header.child_count as usize != self.child_page_ids.len()
                    || !self.leaf_values.is_empty()
                    || self.child_page_ids.len() != self.keys.len() + 1
                {
                    return Err(btree_node_format_error(
                        "BTree internal node key/value/child counts are inconsistent",
                    ));
                }
                if self.child_page_ids.iter().any(|page_id| page_id.is_zero()) {
                    return Err(btree_node_format_error(
                        "BTree internal child page ids must not be zero",
                    ));
                }
                if checked_free_start(internal_body_len(&self.keys, &self.child_page_ids)?)?
                    != self.header.free_start
                {
                    return Err(btree_node_format_error(
                        "BTree internal node free_start does not match encoded body",
                    ));
                }
                if self.header.high_key_offset
                    != high_key_offset_for_internal(self.child_page_ids.len(), &self.keys)
                {
                    return Err(btree_node_format_error(
                        "BTree internal high_key_offset does not match encoded keys",
                    ));
                }
            }
        }
        Ok(())
    }

    fn encode_header(&self) -> AndromedaResult<[u8; BTREE_NODE_V1_HEADER_LEN]> {
        self.header.validate()?;
        Ok(encode_header_without_validation(&self.header))
    }
}
