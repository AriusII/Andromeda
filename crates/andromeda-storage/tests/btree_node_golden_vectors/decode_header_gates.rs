use crate::support::{encode_node, owner_leaf_node};
use andromeda_storage as storage;

#[test]
fn storage_facade_rejects_invalid_magic_through_owner_type() {
    let node = owner_leaf_node(82, 10, Vec::new(), None, None);
    let mut encoded = encode_node(&node);
    encoded[0..4].copy_from_slice(&0u32.to_le_bytes());

    let err = storage::BTreeNodeV1::decode(&encoded).expect_err("invalid magic must be rejected");
    assert!(err.message().contains("magic"));
}

#[test]
fn storage_facade_rejects_unsupported_version_through_owner_type() {
    let node = owner_leaf_node(83, 10, Vec::new(), None, None);
    let mut encoded = encode_node(&node);
    encoded[4..6].copy_from_slice(&2u16.to_le_bytes());

    let err =
        storage::BTreeNodeV1::decode(&encoded).expect_err("unsupported version must be rejected");
    assert!(err.message().contains("format version"));
}

#[test]
fn storage_facade_keeps_page_id_gate() {
    let node = owner_leaf_node(84, 10, Vec::new(), None, None);
    let encoded = encode_node(&node);

    let err = storage::BTreeNodeV1::decode_for_page_id(&encoded, storage::PageId::new(85))
        .expect_err("physical page id mismatch must be rejected");
    assert!(err.message().contains("expected page id"));

    let err = storage::BTreeNodeV1::decode_for_page_id(&encoded, storage::PageId::new(0))
        .expect_err("zero expected page id must be rejected");
    assert!(err.message().contains("must not be zero"));
}
