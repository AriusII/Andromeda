use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{Lsn, PageId};

pub const BTREE_NODE_V1_MAGIC: u32 = 0x5442_4E41;
pub const BTREE_NODE_V1_FORMAT_VERSION: u16 = 1;
pub const BTREE_NODE_V1_HEADER_LEN: usize = 64;
const BTREE_NODE_V1_HEADER_LEN_U16: u16 = BTREE_NODE_V1_HEADER_LEN as u16;
const NONE_PAGE_ID: u64 = 0;
const NO_OFFSET: u16 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreeNodeKindV1 {
    Leaf,
    Internal,
}

impl BTreeNodeKindV1 {
    const fn tag(self) -> u8 {
        match self {
            Self::Leaf => 1,
            Self::Internal => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BTreeNodeHeaderV1 {
    pub magic: u32,
    pub format_version: u16,
    pub node_kind: BTreeNodeKindV1,
    pub page_id: PageId,
    pub page_lsn: Lsn,
    pub key_count: u16,
    pub child_count: u16,
    pub free_start: u16,
    pub free_end: u16,
    pub prev_leaf: Option<PageId>,
    pub next_leaf: Option<PageId>,
    pub high_key_offset: Option<u16>,
    pub header_crc: u32,
}

impl BTreeNodeHeaderV1 {
    pub fn new(
        node_kind: BTreeNodeKindV1,
        page_id: PageId,
        page_lsn: Lsn,
        key_count: u16,
        child_count: u16,
        free_start: u16,
        free_end: u16,
    ) -> Self {
        Self {
            magic: BTREE_NODE_V1_MAGIC,
            format_version: BTREE_NODE_V1_FORMAT_VERSION,
            node_kind,
            page_id,
            page_lsn,
            key_count,
            child_count,
            free_start,
            free_end,
            prev_leaf: None,
            next_leaf: None,
            high_key_offset: None,
            header_crc: 0,
        }
    }

    pub fn with_leaf_links(mut self, prev_leaf: Option<PageId>, next_leaf: Option<PageId>) -> Self {
        self.prev_leaf = prev_leaf;
        self.next_leaf = next_leaf;
        self
    }

    pub fn with_high_key_offset(mut self, high_key_offset: Option<u16>) -> Self {
        self.high_key_offset = high_key_offset;
        self
    }

    pub fn with_computed_crc(mut self) -> AndromedaResult<Self> {
        self.header_crc = 0;
        let crc = header_crc32(&encode_header_without_validation(&self));
        self.header_crc = if crc == 0 { 1 } else { crc };
        self.validate()?;
        Ok(self)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.magic != BTREE_NODE_V1_MAGIC {
            return Err(btree_node_format_error("BTree node V1 magic mismatch"));
        }
        if self.format_version != BTREE_NODE_V1_FORMAT_VERSION {
            return Err(btree_node_format_error(
                "unsupported BTree node V1 format version",
            ));
        }
        if self.page_id.is_zero() {
            return Err(btree_node_format_error(
                "BTree node page_id must not be zero",
            ));
        }
        if self.page_lsn.is_zero() {
            return Err(btree_node_format_error(
                "BTree node page_lsn must not be zero",
            ));
        }
        if self.free_start < BTREE_NODE_V1_HEADER_LEN_U16 {
            return Err(btree_node_format_error(
                "BTree node free_start overlaps fixed header",
            ));
        }
        if self.free_start > self.free_end {
            return Err(btree_node_format_error(
                "BTree node free_start must be <= free_end",
            ));
        }
        if matches!(self.prev_leaf, Some(prev) if prev == self.page_id)
            || matches!(self.next_leaf, Some(next) if next == self.page_id)
        {
            return Err(btree_node_format_error(
                "BTree leaf links must not point to self",
            ));
        }
        if matches!((self.prev_leaf, self.next_leaf), (Some(prev), Some(next)) if prev == next) {
            return Err(btree_node_format_error(
                "BTree previous and next leaf links must differ",
            ));
        }
        match self.node_kind {
            BTreeNodeKindV1::Leaf => {
                if self.child_count != 0 {
                    return Err(btree_node_format_error(
                        "BTree leaf node must not advertise child pointers",
                    ));
                }
            }
            BTreeNodeKindV1::Internal => {
                let expected = self.key_count.checked_add(1).ok_or_else(|| {
                    btree_node_format_error("BTree internal child count overflow")
                })?;
                if self.child_count != expected {
                    return Err(btree_node_format_error(
                        "BTree internal child_count must equal key_count + 1",
                    ));
                }
                if self.prev_leaf.is_some() || self.next_leaf.is_some() {
                    return Err(btree_node_format_error(
                        "BTree internal nodes must not carry leaf sibling links",
                    ));
                }
            }
        }
        if let Some(high_key_offset) = self.high_key_offset
            && (high_key_offset < BTREE_NODE_V1_HEADER_LEN_U16
                || high_key_offset >= self.free_start)
        {
            return Err(btree_node_format_error(
                "BTree high_key_offset must point inside encoded key area",
            ));
        }

        let mut expected_header = self.clone();
        expected_header.header_crc = 0;
        let expected = header_crc32(&encode_header_without_validation(&expected_header));
        let expected = if expected == 0 { 1 } else { expected };
        if self.header_crc != expected {
            return Err(btree_node_format_error("BTree node header CRC mismatch"));
        }
        Ok(())
    }
}

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

fn decode_header(bytes: &[u8]) -> AndromedaResult<BTreeNodeHeaderV1> {
    if bytes.len() < BTREE_NODE_V1_HEADER_LEN {
        return Err(btree_node_format_error("BTree node header is truncated"));
    }
    let magic = read_u32(bytes, 0)?;
    if magic != BTREE_NODE_V1_MAGIC {
        return Err(btree_node_format_error(
            "BTree node V1 little-endian magic mismatch",
        ));
    }
    let format_version = read_u16(bytes, 4)?;
    if format_version != BTREE_NODE_V1_FORMAT_VERSION {
        return Err(btree_node_format_error(
            "unsupported BTree node V1 format version",
        ));
    }
    let node_kind = match bytes[6] {
        1 => BTreeNodeKindV1::Leaf,
        2 => BTreeNodeKindV1::Internal,
        _ => return Err(btree_node_format_error("unknown BTree node kind tag")),
    };
    if bytes[7] != 0 {
        return Err(btree_node_format_error(
            "BTree node reserved header byte must be zero",
        ));
    }
    if read_u16(bytes, 50)? != 0 {
        return Err(btree_node_format_error(
            "BTree node reserved header field must be zero",
        ));
    }
    let header = BTreeNodeHeaderV1 {
        magic,
        format_version,
        node_kind,
        page_id: PageId::new(read_u64(bytes, 8)?),
        page_lsn: Lsn::new(read_u64(bytes, 16)?),
        key_count: read_u16(bytes, 24)?,
        child_count: read_u16(bytes, 26)?,
        free_start: read_u16(bytes, 28)?,
        free_end: read_u16(bytes, 30)?,
        prev_leaf: optional_page_id(read_u64(bytes, 32)?),
        next_leaf: optional_page_id(read_u64(bytes, 40)?),
        high_key_offset: optional_offset(read_u16(bytes, 48)?),
        header_crc: read_u32(bytes, 52)?,
    };
    header.validate()?;
    Ok(header)
}

fn encode_header_without_validation(header: &BTreeNodeHeaderV1) -> [u8; BTREE_NODE_V1_HEADER_LEN] {
    let mut bytes = [0u8; BTREE_NODE_V1_HEADER_LEN];
    write_u32(&mut bytes, 0, header.magic);
    write_u16(&mut bytes, 4, header.format_version);
    bytes[6] = header.node_kind.tag();
    bytes[7] = 0;
    write_u64(&mut bytes, 8, header.page_id.get());
    write_u64(&mut bytes, 16, header.page_lsn.get());
    write_u16(&mut bytes, 24, header.key_count);
    write_u16(&mut bytes, 26, header.child_count);
    write_u16(&mut bytes, 28, header.free_start);
    write_u16(&mut bytes, 30, header.free_end);
    write_u64(
        &mut bytes,
        32,
        header.prev_leaf.map_or(NONE_PAGE_ID, PageId::get),
    );
    write_u64(
        &mut bytes,
        40,
        header.next_leaf.map_or(NONE_PAGE_ID, PageId::get),
    );
    write_u16(&mut bytes, 48, header.high_key_offset.unwrap_or(NO_OFFSET));
    write_u16(&mut bytes, 50, 0);
    write_u32(&mut bytes, 52, header.header_crc);
    bytes
}

fn validate_key_order(keys: &[Vec<u8>]) -> AndromedaResult<()> {
    for pair in keys.windows(2) {
        if pair[0] >= pair[1] {
            return Err(btree_node_format_error(
                "BTree node keys must be strictly ordered",
            ));
        }
    }
    Ok(())
}

fn leaf_body_len(keys: &[Vec<u8>], values: &[Vec<u8>]) -> AndromedaResult<usize> {
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

fn internal_body_len(keys: &[Vec<u8>], children: &[PageId]) -> AndromedaResult<usize> {
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

fn checked_free_start(body_len: usize) -> AndromedaResult<u16> {
    checked_u16(
        BTREE_NODE_V1_HEADER_LEN
            .checked_add(body_len)
            .ok_or_else(|| btree_node_format_error("BTree node body length overflow"))?,
        "BTree node free_start exceeds u16",
    )
}

fn validate_leaf_body_min_len(key_count: u16, body_len: usize) -> AndromedaResult<()> {
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

fn validate_internal_body_min_len(
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

fn high_key_offset_for_leaf(keys: &[Vec<u8>]) -> Option<u16> {
    if keys.is_empty() {
        return None;
    }
    let mut offset = BTREE_NODE_V1_HEADER_LEN;
    for key in &keys[..keys.len() - 1] {
        offset += 2 + key.len();
    }
    u16::try_from(offset).ok()
}

fn high_key_offset_for_internal(child_count: usize, keys: &[Vec<u8>]) -> Option<u16> {
    if keys.is_empty() {
        return None;
    }
    let mut offset = BTREE_NODE_V1_HEADER_LEN + child_count * 8;
    for key in &keys[..keys.len() - 1] {
        offset += 2 + key.len();
    }
    u16::try_from(offset).ok()
}

fn write_len_prefixed_bytes(
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

fn read_len_prefixed_bytes(
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

fn write_u64_at(target: &mut [u8], offset: &mut usize, value: u64) -> AndromedaResult<()> {
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

fn read_u64_at(source: &[u8], offset: &mut usize, limit: usize) -> AndromedaResult<u64> {
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

fn write_u16(target: &mut [u8], offset: usize, value: u16) {
    target[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(target: &mut [u8], offset: usize, value: u32) {
    target[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(target: &mut [u8], offset: usize, value: u64) {
    target[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
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

fn read_u32(source: &[u8], offset: usize) -> AndromedaResult<u32> {
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(
        source
            .get(offset..offset + 4)
            .ok_or_else(|| btree_node_format_error("BTree node u32 field is truncated"))?,
    );
    Ok(u32::from_le_bytes(bytes))
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

fn optional_page_id(value: u64) -> Option<PageId> {
    if value == NONE_PAGE_ID {
        None
    } else {
        Some(PageId::new(value))
    }
}

fn optional_offset(value: u16) -> Option<u16> {
    if value == NO_OFFSET {
        None
    } else {
        Some(value)
    }
}

fn checked_u16(value: usize, message: &'static str) -> AndromedaResult<u16> {
    u16::try_from(value).map_err(|_| btree_node_format_error(message))
}

fn header_crc32(bytes: &[u8]) -> u32 {
    const FNV_OFFSET: u32 = 0x811C_9DC5;
    const FNV_PRIME: u32 = 0x0100_0193;
    let mut state = FNV_OFFSET;
    for (idx, byte) in bytes.iter().enumerate() {
        let byte = if (52..56).contains(&idx) { 0 } else { *byte };
        state ^= u32::from(byte);
        state = state.wrapping_mul(FNV_PRIME);
    }
    state
}

fn btree_node_format_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
