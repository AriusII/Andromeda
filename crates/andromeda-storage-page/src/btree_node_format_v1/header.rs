use andromeda_error::AndromedaResult;

use crate::{Lsn, PageId};

use super::validation::{btree_node_format_error, validate_header};

pub const BTREE_NODE_V1_MAGIC: u32 = 0x5442_4E41;
pub const BTREE_NODE_V1_FORMAT_VERSION: u16 = 1;
pub const BTREE_NODE_V1_HEADER_LEN: usize = 64;
pub(super) const BTREE_NODE_V1_HEADER_LEN_U16: u16 = BTREE_NODE_V1_HEADER_LEN as u16;
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
        validate_header(self)
    }
}

pub(super) fn decode_header(bytes: &[u8]) -> AndromedaResult<BTreeNodeHeaderV1> {
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

pub(super) fn encode_header_without_validation(
    header: &BTreeNodeHeaderV1,
) -> [u8; BTREE_NODE_V1_HEADER_LEN] {
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

pub(super) fn header_crc32(bytes: &[u8]) -> u32 {
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
