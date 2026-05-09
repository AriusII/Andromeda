use crate::support::{empty_leaf, encode_node, internal_node, leaf_entry, leaf_node};
use andromeda_storage_page::{BTREE_NODE_V1_MAGIC, BTreeNodeV1, PageId};

#[test]
fn invalid_magic_is_rejected_from_decoded_header() {
    let node = empty_leaf(82, 10);
    let mut encoded = encode_node(&node);
    encoded[0..4].copy_from_slice(&0u32.to_le_bytes());

    let err = BTreeNodeV1::decode(&encoded).expect_err("invalid magic must be rejected");
    assert!(err.message().contains("magic"));
}

#[test]
fn unsupported_version_is_rejected_from_decoded_header() {
    let node = empty_leaf(83, 10);
    let mut encoded = encode_node(&node);
    encoded[4..6].copy_from_slice(&2u16.to_le_bytes());

    let err = BTreeNodeV1::decode(&encoded).expect_err("unsupported version must be rejected");
    assert!(err.message().contains("format version"));
}

#[test]
fn magic_is_checked_before_secondary_header_fields() {
    let node = empty_leaf(830, 10);
    let mut encoded = encode_node(&node);
    encoded[0..4].copy_from_slice(&BTREE_NODE_V1_MAGIC.to_be_bytes());
    encoded[6] = 0xFF;

    let err = BTreeNodeV1::decode(&encoded).expect_err("wrong-endian magic must be rejected");
    assert!(err.message().contains("magic"));
}

#[test]
fn reserved_header_bytes_are_rejected_from_decoded_header() {
    let node = empty_leaf(831, 10);
    let mut encoded = encode_node(&node);
    encoded[7] = 1;
    let err = BTreeNodeV1::decode(&encoded).expect_err("reserved byte must be rejected");
    assert!(err.message().contains("reserved header byte"));

    let mut encoded = encode_node(&node);
    encoded[50..52].copy_from_slice(&1u16.to_le_bytes());
    let err = BTreeNodeV1::decode(&encoded).expect_err("reserved field must be rejected");
    assert!(err.message().contains("reserved header field"));
}

#[test]
fn page_id_mismatch_is_rejected_against_physical_page_id() {
    let node = empty_leaf(84, 10);
    let encoded = encode_node(&node);

    let err = BTreeNodeV1::decode_for_page_id(&encoded, PageId::new(85))
        .expect_err("physical page id mismatch must be rejected");
    assert!(err.message().contains("expected page id"));

    let err = BTreeNodeV1::decode_for_page_id(&encoded, PageId::new(0))
        .expect_err("zero expected page id must be rejected");
    assert!(err.message().contains("must not be zero"));
}

#[test]
fn invalid_leaf_child_count_is_rejected_from_decoded_header() {
    let node = empty_leaf(86, 10);
    let mut encoded = encode_node(&node);
    encoded[26..28].copy_from_slice(&1u16.to_le_bytes());

    let err = BTreeNodeV1::decode(&encoded).expect_err("leaf child count must be rejected");
    assert!(err.message().contains("child pointers"));
}

#[test]
fn invalid_internal_child_count_is_rejected_from_decoded_header() {
    let node = internal_node(87, 13, vec![b"k10".to_vec()], vec![100, 101]);
    let mut encoded = encode_node(&node);
    encoded[26..28].copy_from_slice(&1u16.to_le_bytes());

    let err = BTreeNodeV1::decode(&encoded).expect_err("bad child count must be rejected");
    assert!(err.message().contains("key_count + 1"));
}

#[test]
fn leaf_sibling_pointer_violations_are_rejected_from_decoded_header() {
    let node = leaf_node(88, 14, Vec::new(), Some(87), Some(89));

    let mut encoded = encode_node(&node);
    encoded[32..40].copy_from_slice(&88u64.to_le_bytes());
    let err = BTreeNodeV1::decode(&encoded).expect_err("self sibling link must be rejected");
    assert!(err.message().contains("point to self"));

    let mut encoded = encode_node(&node);
    encoded[32..40].copy_from_slice(&90u64.to_le_bytes());
    encoded[40..48].copy_from_slice(&90u64.to_le_bytes());
    let err = BTreeNodeV1::decode(&encoded).expect_err("duplicate sibling links must be rejected");
    assert!(err.message().contains("must differ"));
}

#[test]
fn internal_sibling_pointers_are_rejected_from_decoded_header() {
    let node = internal_node(89, 15, vec![b"k10".to_vec()], vec![100, 101]);
    let mut encoded = encode_node(&node);
    encoded[32..40].copy_from_slice(&88u64.to_le_bytes());

    let err = BTreeNodeV1::decode(&encoded).expect_err("internal sibling link must be rejected");
    assert!(err.message().contains("internal nodes"));
}

#[test]
fn header_crc_detects_corrupted_node_header() {
    let node = leaf_node(91, 12, vec![leaf_entry(b"a", 10)], None, None);
    let mut encoded = encode_node(&node);
    encoded[8] ^= 0xFF;

    let err = BTreeNodeV1::decode(&encoded).expect_err("corrupted header must be rejected");
    assert!(err.message().contains("CRC") || err.message().contains("page_id"));
}

#[test]
fn header_crc_field_corruption_is_rejected() {
    let node = leaf_node(98, 19, vec![leaf_entry(b"a", 10)], None, None);
    let mut encoded = encode_node(&node);
    encoded[52] ^= 0xAA;

    let err = BTreeNodeV1::decode(&encoded).expect_err("corrupted CRC must be rejected");
    assert!(err.message().contains("CRC"));
}
