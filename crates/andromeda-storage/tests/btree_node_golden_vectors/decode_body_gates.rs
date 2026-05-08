use crate::support::{encode_node, leaf_entry, owner_internal_node, owner_leaf_node};
use andromeda_storage as storage;

#[test]
fn storage_facade_rejects_duplicate_keys_from_owner_image() {
    let node = owner_leaf_node(
        81,
        10,
        vec![leaf_entry(b"a", 10), leaf_entry(b"b", 11)],
        None,
        None,
    );
    let mut encoded = encode_node(&node);
    let second_key_byte_offset = storage::BTREE_NODE_V1_HEADER_LEN + 2 + 1 + 2 + 8 + 2;
    encoded[second_key_byte_offset] = b'a';

    let err = storage::BTreeNodeV1::decode(&encoded)
        .expect_err("duplicate decoded keys must be rejected");
    assert!(err.message().contains("strictly ordered"));
}

#[test]
fn storage_facade_rejects_zero_child_pointer_from_owner_image() {
    let node = owner_internal_node(92, 13, vec![b"k10".to_vec()], vec![100, 101]);
    let mut encoded = encode_node(&node);
    encoded[storage::BTREE_NODE_V1_HEADER_LEN..storage::BTREE_NODE_V1_HEADER_LEN + 8]
        .copy_from_slice(&0u64.to_le_bytes());

    let err =
        storage::BTreeNodeV1::decode(&encoded).expect_err("zero child pointer must be rejected");
    assert!(err.message().contains("child page ids must not be zero"));
}
