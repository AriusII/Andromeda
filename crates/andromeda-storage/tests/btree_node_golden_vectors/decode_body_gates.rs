use crate::support::{
    empty_leaf, encode_node, internal_node, leaf_entry, leaf_node, refresh_btree_header_crc,
};
use andromeda_storage::{BTREE_NODE_V1_HEADER_LEN, BTreeNodeV1};

#[test]
fn impossible_leaf_key_count_is_rejected_before_body_decode() {
    let node = empty_leaf(832, 10);
    let mut encoded = encode_node(&node);
    encoded[24..26].copy_from_slice(&1u16.to_le_bytes());
    refresh_btree_header_crc(&mut encoded);

    let err = BTreeNodeV1::decode(&encoded).expect_err("body length gate must reject key count");
    assert!(err.message().contains("key_count"));
    assert!(err.message().contains("encoded body length"));
}

#[test]
fn duplicate_keys_are_rejected_from_decoded_body() {
    let node = leaf_node(
        81,
        10,
        vec![leaf_entry(b"a", 10), leaf_entry(b"b", 11)],
        None,
        None,
    );
    let mut encoded = encode_node(&node);
    let second_key_byte_offset = BTREE_NODE_V1_HEADER_LEN + 2 + 1 + 2 + 8 + 2;
    encoded[second_key_byte_offset] = b'a';

    let err = BTreeNodeV1::decode(&encoded).expect_err("duplicate decoded keys must be rejected");
    assert!(err.message().contains("strictly ordered"));
}

#[test]
fn internal_zero_child_pointer_is_rejected_from_decoded_body() {
    let node = internal_node(92, 13, vec![b"k10".to_vec()], vec![100, 101]);
    let mut encoded = encode_node(&node);
    encoded[BTREE_NODE_V1_HEADER_LEN..BTREE_NODE_V1_HEADER_LEN + 8]
        .copy_from_slice(&0u64.to_le_bytes());

    let err = BTreeNodeV1::decode(&encoded).expect_err("zero child pointer must be rejected");
    assert!(err.message().contains("child page ids must not be zero"));
}

#[test]
fn truncated_length_prefixed_key_is_rejected() {
    let node = leaf_node(96, 17, vec![leaf_entry(b"a", 10)], None, None);
    let mut encoded = encode_node(&node);
    encoded[BTREE_NODE_V1_HEADER_LEN..BTREE_NODE_V1_HEADER_LEN + 2]
        .copy_from_slice(&4096u16.to_le_bytes());

    let err = BTreeNodeV1::decode(&encoded).expect_err("truncated key must be rejected");
    assert!(err.message().contains("free_start"));
}

#[test]
fn truncated_length_prefixed_value_is_rejected() {
    let node = leaf_node(97, 18, vec![leaf_entry(b"a", 10)], None, None);
    let mut encoded = encode_node(&node);
    let value_len_offset = BTREE_NODE_V1_HEADER_LEN + 2 + 1;
    encoded[value_len_offset..value_len_offset + 2].copy_from_slice(&4096u16.to_le_bytes());

    let err = BTreeNodeV1::decode(&encoded).expect_err("truncated value must be rejected");
    assert!(err.message().contains("free_start"));
}
