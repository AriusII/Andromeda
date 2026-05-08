use crate::support::{encode_node, leaf_entry, owner_internal_node, owner_leaf_node};
use andromeda_storage as storage;
use andromeda_storage_page as page;

#[test]
fn storage_facade_decodes_owner_leaf_full_image() {
    let node = owner_leaf_node(
        150,
        22,
        vec![leaf_entry(b"aa", 100), leaf_entry(b"bb", 101)],
        Some(149),
        Some(151),
    );
    let encoded = encode_node(&node);

    let decoded = storage::BTreeNodeV1::decode(&encoded).expect("facade decodes owner leaf image");
    assert_eq!(decoded, node);
    assert_eq!(decoded.header.node_kind, storage::BTreeNodeKindV1::Leaf);
    assert_eq!(decoded.header.high_key_offset, Some(68));
    assert_eq!(
        storage::BTreeNodeV1::decode_for_page_id(&encoded, storage::PageId::new(150))
            .expect("facade page id gate accepts owner image"),
        node
    );
}

#[test]
fn storage_facade_decodes_owner_internal_full_image() {
    let node = owner_internal_node(
        160,
        23,
        vec![b"k10".to_vec(), b"k20".to_vec()],
        vec![200, 201, 202],
    );
    let encoded = encode_node(&node);

    let decoded =
        storage::BTreeNodeV1::decode(&encoded).expect("facade decodes owner internal image");
    assert_eq!(decoded, node);
    assert_eq!(decoded.header.node_kind, page::BTreeNodeKindV1::Internal);
    assert_eq!(decoded.header.high_key_offset, Some(93));
    assert_eq!(
        decoded.child_page_ids,
        vec![
            storage::PageId::new(200),
            storage::PageId::new(201),
            storage::PageId::new(202)
        ]
    );
    assert_eq!(
        storage::BTreeNodeV1::decode_for_page_id(&encoded, page::PageId::new(160))
            .expect("facade page id gate accepts owner image"),
        node
    );
}
