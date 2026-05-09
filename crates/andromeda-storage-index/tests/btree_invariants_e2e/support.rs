use andromeda_storage_index::{BTreeConfig, BTreeNodeImpl, KeyValuePair};

pub(crate) fn row_id_value(kvp: &KeyValuePair) -> u64 {
    let bytes: [u8; 8] = kvp
        .value
        .as_slice()
        .try_into()
        .expect("BTreeNodeImpl leaf values store RowId as 8 little-endian bytes");
    u64::from_le_bytes(bytes)
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

pub(crate) fn split_test_config() -> BTreeConfig {
    BTreeConfig {
        branching_factor: 8,
        ..BTreeConfig::default()
    }
}
