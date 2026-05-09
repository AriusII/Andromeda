#![allow(dead_code)]

use andromeda_storage_index::{
    BTreeConfig, BTreeNodeImpl, InMemoryBTreeIndexEngine, IndexId, KeyValuePair, PageId, RowId,
};

pub(crate) fn default_engine() -> InMemoryBTreeIndexEngine {
    InMemoryBTreeIndexEngine::new(IndexId::new(1), PageId::new(10), BTreeConfig::default())
}

pub(crate) fn leaf_node(page_id: u64, parent_page_id: Option<u64>) -> BTreeNodeImpl {
    BTreeNodeImpl::new_leaf(PageId::new(page_id), parent_page_id.map(PageId::new))
}

pub(crate) fn internal_node(page_id: u64, parent_page_id: Option<u64>) -> BTreeNodeImpl {
    BTreeNodeImpl::new_internal(PageId::new(page_id), parent_page_id.map(PageId::new))
}

pub(crate) fn empty_value_pair(key: u8) -> KeyValuePair {
    KeyValuePair {
        key: vec![key],
        value: Vec::new(),
    }
}

pub(crate) fn insert_leaf_keys(node: &mut BTreeNodeImpl, keys: impl IntoIterator<Item = u8>) {
    for key in keys {
        node.insert_into_leaf(vec![key], RowId::new(key as u64))
            .expect("test fixture keys are unique and within bounds");
    }
}

pub(crate) fn fill_leaf_to_capacity(node: &mut BTreeNodeImpl, config: &BTreeConfig) {
    for key in 0..(config.branching_factor - 1) {
        node.insert_into_leaf(vec![key as u8], RowId::new(key as u64))
            .expect("capacity fixture inserts unique keys");
    }
}

pub(crate) fn fill_internal_to_capacity(node: &mut BTreeNodeImpl, config: &BTreeConfig) {
    for key in 0..(config.branching_factor - 1) {
        node.key_value_pairs.push(empty_value_pair(key as u8));
    }
    for child in 0..config.branching_factor {
        node.child_page_ids.push(PageId::new((child as u64) * 100));
    }
}

pub(crate) fn row_id_from_value(kvp: &KeyValuePair) -> RowId {
    let bytes: [u8; 8] = kvp
        .value
        .as_slice()
        .try_into()
        .expect("BTreeNodeImpl leaf values store RowId as 8 little-endian bytes");
    RowId::new(u64::from_le_bytes(bytes))
}

pub(crate) fn assert_leaf_keys_strictly_ordered(node: &BTreeNodeImpl) {
    assert!(node.is_leaf, "expected a leaf node");
    for pair in node.key_value_pairs.windows(2) {
        assert!(
            pair[0].key < pair[1].key,
            "leaf keys must be strictly increasing: {:?} then {:?}",
            pair[0].key,
            pair[1].key
        );
    }
}
