//! B+ Tree node management — load, create, serialize, deserialize.
use super::*;

const NODE_IMPL_LEAF_TAG: u8 = 1;
const NODE_IMPL_INTERNAL_TAG: u8 = 0;
const NODE_IMPL_ABSENT_PAGE_ID: u64 = u64::MAX;
const NODE_IMPL_HEADER_LEN: usize = 1 + 2 + 2 + 8 + 8;

impl BTreeIndexNode {
    /// Create a new internal node (leaf=false).
    pub fn new_internal(node_id: PageId, parent_id: PageId) -> Self {
        Self {
            node_id,
            is_leaf: false,
            key_count: 0,
            parent_id,
            keys: Vec::new(),
            children: Vec::new(),
            values: Vec::new(),
        }
    }

    /// Create a new leaf node (leaf=true).
    pub fn new_leaf(node_id: PageId, parent_id: PageId) -> Self {
        Self {
            node_id,
            is_leaf: true,
            key_count: 0,
            parent_id,
            keys: Vec::new(),
            children: Vec::new(),
            values: Vec::new(),
        }
    }

    /// Check if node is at capacity (branching_factor keys).
    pub fn is_full(&self, branching_factor: u16) -> bool {
        self.key_count as usize >= branching_factor.saturating_sub(1) as usize
    }

    /// Check if node is below minimum occupancy threshold.
    pub fn is_underfull(&self, branching_factor: u16) -> bool {
        self.key_count as usize <= non_root_min_keys(branching_factor).saturating_sub(1)
    }

    /// Find child pointer for a given key using binary search.
    /// Returns the index of the child pointer that should contain the key.
    pub fn find_child(&self, key: &[u8]) -> PageId {
        let child_idx = self
            .keys
            .partition_point(|existing_key| existing_key.as_slice() <= key);
        self.children
            .get(child_idx)
            .copied()
            .or_else(|| self.children.last().copied())
            .unwrap_or_else(|| PageId::new(0))
    }

    /// Split this node and return the promoted key and new right sibling ID.
    ///
    /// For leaf: moves right half of keys/values to new node, promotes middle key.
    /// For internal: moves right half of keys/children to new node, promotes middle key.
    pub fn split(&mut self, _branching_factor: u16) -> AndromedaResult<(Vec<u8>, PageId)> {
        deferred_btree_result("page-backed node split")
    }

    /// Merge this node with a sibling (assumes both underfull).
    pub fn merge(&mut self, _sibling: &BTreeIndexNode) -> AndromedaResult<()> {
        deferred_btree_result("page-backed node merge")
    }

    /// Serialize node to page buffer.
    pub fn serialize_to_page(&self, _page_buffer: &mut [u8]) -> AndromedaResult<()> {
        deferred_btree_result("page-backed node serialization")
    }

    /// Deserialize node from page buffer.
    pub fn deserialize_from_page(_page_buffer: &[u8]) -> AndromedaResult<Self> {
        deferred_btree_result("page-backed node deserialization")
    }
}

/// Node lifecycle operations (design only).
impl BTreeIndexNode {
    /// Load a node from a page in the buffer pool.
    pub fn load_from_buffer_pool(_page_id: PageId) -> AndromedaResult<Self> {
        deferred_btree_result("load node from buffer pool")
    }

    /// Flush node changes back to buffer pool.
    pub fn flush_to_buffer_pool(&self, _page_id: PageId) -> AndromedaResult<()> {
        deferred_btree_result("flush node to buffer pool")
    }

    /// Validate node structure invariants (for recovery/debugging).
    pub fn validate(&self, branching_factor: u16) -> AndromedaResult<()> {
        if self.key_count as usize != self.keys.len() {
            return Err(BTreeError::CorruptedNode {
                node_id: self.node_id,
                reason: "key_count does not match key vector length".to_string(),
            }
            .into());
        }

        if self.is_leaf {
            if self.values.len() != self.keys.len() {
                return Err(BTreeError::CorruptedNode {
                    node_id: self.node_id,
                    reason: "leaf value vector length does not match key_count".to_string(),
                }
                .into());
            }
        } else if !self.keys.is_empty() && self.children.len() != self.keys.len() + 1 {
            return Err(BTreeError::CorruptedNode {
                node_id: self.node_id,
                reason: "internal child pointer count must be key_count + 1".to_string(),
            }
            .into());
        }

        if self.is_full(branching_factor) && branching_factor <= 1 {
            return Err(BTreeError::CorruptedNode {
                node_id: self.node_id,
                reason: "branching_factor must be greater than one".to_string(),
            }
            .into());
        }

        Ok(())
    }
}

impl BTreeNodeImpl {
    /// Create a new leaf node
    pub fn new_leaf(page_id: PageId, parent_page_id: Option<PageId>) -> Self {
        Self {
            page_id,
            is_leaf: true,
            parent_page_id,
            next_sibling_page_id: None,
            key_value_pairs: Vec::new(),
            child_page_ids: Vec::new(),
        }
    }

    /// Create a new internal node
    pub fn new_internal(page_id: PageId, parent_page_id: Option<PageId>) -> Self {
        Self {
            page_id,
            is_leaf: false,
            parent_page_id,
            next_sibling_page_id: None,
            key_value_pairs: Vec::new(),
            child_page_ids: Vec::new(),
        }
    }

    /// Check if node is at capacity
    pub fn is_full(&self, branching_factor: u16) -> bool {
        self.key_value_pairs.len() >= (branching_factor - 1) as usize
    }

    /// Check if node is below minimum occupancy (except root)
    pub fn is_underfull(&self, branching_factor: u16) -> bool {
        let min_keys = if self.parent_page_id.is_some() {
            (branching_factor / 2 - 1) as usize
        } else {
            1 // Root can have just 1 key
        };
        self.key_value_pairs.len() < min_keys
    }

    /// Binary search for a key; returns index where key should be inserted
    pub fn find_key_index(&self, key: &[u8]) -> usize {
        self.key_value_pairs
            .binary_search_by(|kvp| kvp.key.as_slice().cmp(key))
            .unwrap_or_else(|idx| idx)
    }

    /// Find the child pointer index for a given key (internal nodes only)
    pub fn find_child_index(&self, key: &[u8]) -> usize {
        if self.is_leaf {
            return 0;
        }

        let idx = self
            .key_value_pairs
            .binary_search_by(|kvp| kvp.key.as_slice().cmp(key))
            .unwrap_or_else(|idx| idx);

        // If key matches a key in this node, go to right child
        if idx < self.key_value_pairs.len() && self.key_value_pairs[idx].key == key {
            idx + 1
        } else {
            idx
        }
    }

    /// Get the child page ID at given index
    pub fn get_child_page_id(&self, index: usize) -> Option<PageId> {
        if index < self.child_page_ids.len() {
            Some(self.child_page_ids[index])
        } else {
            None
        }
    }

    /// Split this node and return (promoted_key, new_node_data)
    pub fn split(&mut self, branching_factor: u16) -> AndromedaResult<(Vec<u8>, BTreeNodeImpl)> {
        if !self.is_full(branching_factor) {
            return Err(BTreeError::CorruptedNode {
                node_id: self.page_id,
                reason: "split called on non-full node".to_string(),
            }
            .into());
        }

        let mid_idx = self.key_value_pairs.len() / 2;
        let promoted_key = self.key_value_pairs[mid_idx].key.clone();

        // Create new right sibling
        let mut new_node = if self.is_leaf {
            BTreeNodeImpl::new_leaf(
                PageId::new(self.page_id.get().wrapping_add(1000)),
                self.parent_page_id,
            )
        } else {
            BTreeNodeImpl::new_internal(
                PageId::new(self.page_id.get().wrapping_add(1000)),
                self.parent_page_id,
            )
        };

        if self.is_leaf {
            // Split leaf: move right half to new node
            new_node.key_value_pairs = self.key_value_pairs[mid_idx..].to_vec();
            self.key_value_pairs.truncate(mid_idx);

            // Update leaf linking
            new_node.next_sibling_page_id = self.next_sibling_page_id;
            self.next_sibling_page_id = Some(new_node.page_id);
        } else {
            // Split internal: move right half of keys and all right children
            new_node.key_value_pairs = self.key_value_pairs[mid_idx + 1..].to_vec();
            self.key_value_pairs.truncate(mid_idx);

            new_node.child_page_ids = self.child_page_ids[mid_idx + 1..].to_vec();
            self.child_page_ids.truncate(mid_idx + 1);
        }

        Ok((promoted_key, new_node))
    }

    /// Serialize node to bytes for page storage
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Header: is_leaf, key_count, child_count
        bytes.push(if self.is_leaf {
            NODE_IMPL_LEAF_TAG
        } else {
            NODE_IMPL_INTERNAL_TAG
        });
        let key_count = u16::try_from(self.key_value_pairs.len())
            .expect("BTreeNodeImpl key count must fit u16");
        bytes.extend_from_slice(&key_count.to_le_bytes());
        let child_count = u16::try_from(self.child_page_ids.len())
            .expect("BTreeNodeImpl child count must fit u16");
        bytes.extend_from_slice(&child_count.to_le_bytes());

        // Parent and next sibling page IDs
        let parent_id = self
            .parent_page_id
            .map(PageId::get)
            .unwrap_or(NODE_IMPL_ABSENT_PAGE_ID);
        bytes.extend_from_slice(&parent_id.to_le_bytes());
        let next_sibling = self
            .next_sibling_page_id
            .map(PageId::get)
            .unwrap_or(NODE_IMPL_ABSENT_PAGE_ID);
        bytes.extend_from_slice(&next_sibling.to_le_bytes());

        // Key-value pairs
        for kvp in &self.key_value_pairs {
            let key_len =
                u16::try_from(kvp.key.len()).expect("BTreeNodeImpl key length must fit u16");
            bytes.extend_from_slice(&key_len.to_le_bytes());
            bytes.extend_from_slice(&kvp.key);

            let val_len =
                u16::try_from(kvp.value.len()).expect("BTreeNodeImpl value length must fit u16");
            bytes.extend_from_slice(&val_len.to_le_bytes());
            bytes.extend_from_slice(&kvp.value);
        }

        // Child page IDs
        for child_id in &self.child_page_ids {
            bytes.extend_from_slice(&child_id.get().to_le_bytes());
        }

        bytes
    }

    /// Deserialize node from bytes
    pub fn deserialize(page_id: PageId, data: &[u8]) -> AndromedaResult<Self> {
        let mut offset = 0;
        let node_tag = read_node_u8(page_id, data, &mut offset)?;
        let is_leaf = match node_tag {
            NODE_IMPL_LEAF_TAG => true,
            NODE_IMPL_INTERNAL_TAG => false,
            _ => return invalid_node_format(page_id),
        };

        let key_count = usize::from(read_node_u16(page_id, data, &mut offset)?);

        let child_count = usize::from(read_node_u16(page_id, data, &mut offset)?);

        let parent_id_raw = read_node_u64(page_id, data, &mut offset)?;
        let parent_page_id = if parent_id_raw == NODE_IMPL_ABSENT_PAGE_ID {
            None
        } else {
            Some(PageId::new(parent_id_raw))
        };

        let next_sibling_raw = read_node_u64(page_id, data, &mut offset)?;
        let next_sibling_page_id = if next_sibling_raw == NODE_IMPL_ABSENT_PAGE_ID {
            None
        } else {
            Some(PageId::new(next_sibling_raw))
        };

        let mut key_value_pairs = Vec::with_capacity(key_count);
        for _ in 0..key_count {
            let key_len = usize::from(read_node_u16(page_id, data, &mut offset)?);
            let key = read_node_bytes(page_id, data, &mut offset, key_len)?.to_vec();

            let val_len = usize::from(read_node_u16(page_id, data, &mut offset)?);
            let value = read_node_bytes(page_id, data, &mut offset, val_len)?.to_vec();

            key_value_pairs.push(KeyValuePair { key, value });
        }

        let mut child_page_ids = Vec::with_capacity(child_count);
        for _ in 0..child_count {
            let child_id = read_node_u64(page_id, data, &mut offset)?;
            child_page_ids.push(PageId::new(child_id));
        }

        Ok(BTreeNodeImpl {
            page_id,
            is_leaf,
            parent_page_id,
            next_sibling_page_id,
            key_value_pairs,
            child_page_ids,
        })
    }
}

fn read_node_u8(page_id: PageId, data: &[u8], offset: &mut usize) -> AndromedaResult<u8> {
    let bytes = read_node_bytes(page_id, data, offset, 1)?;
    Ok(bytes[0])
}

fn read_node_u16(page_id: PageId, data: &[u8], offset: &mut usize) -> AndromedaResult<u16> {
    let bytes = read_node_bytes(page_id, data, offset, 2)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_node_u64(page_id: PageId, data: &[u8], offset: &mut usize) -> AndromedaResult<u64> {
    let bytes = read_node_bytes(page_id, data, offset, 8)?;
    let mut field = [0; 8];
    field.copy_from_slice(bytes);
    Ok(u64::from_le_bytes(field))
}

fn read_node_bytes<'a>(
    page_id: PageId,
    data: &'a [u8],
    offset: &mut usize,
    len: usize,
) -> AndromedaResult<&'a [u8]> {
    if data.len() < NODE_IMPL_HEADER_LEN {
        return invalid_node_format(page_id);
    }
    let end = offset.checked_add(len).ok_or_else(|| {
        andromeda_core::AndromedaError::from(BTreeError::InvalidNodeFormat { page_id })
    })?;
    let bytes = data.get(*offset..end).ok_or_else(|| {
        andromeda_core::AndromedaError::from(BTreeError::InvalidNodeFormat { page_id })
    })?;
    *offset = end;
    Ok(bytes)
}

fn invalid_node_format<T>(page_id: PageId) -> AndromedaResult<T> {
    Err(BTreeError::InvalidNodeFormat { page_id }.into())
}
