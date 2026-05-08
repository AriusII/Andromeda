use super::{BTREE_NODE_V1_HEADER_LEN, BTreeNodeV1};
use crate::{BTreeNodeKindV1, Lsn, PageId};

#[allow(dead_code)]
pub(super) fn assert_header_golden(node: &BTreeNodeV1, expected: &[u8; BTREE_NODE_V1_HEADER_LEN]) {
    let encoded = node.encode().expect("node image encodes");
    assert_eq!(
        &encoded[..BTREE_NODE_V1_HEADER_LEN],
        expected,
        "BTree node V1 header golden bytes drifted"
    );
}

#[test]
fn empty_leaf_header_golden_vector_is_stable() {
    let node = BTreeNodeV1::new_leaf(
        PageId::new(42),
        Lsn::new(7),
        Vec::new(),
        Some(PageId::new(41)),
        Some(PageId::new(43)),
        4096,
    )
    .expect("leaf node builds");
    let encoded = node.encode().expect("leaf node encodes");

    const EMPTY_LEAF_HEADER: [u8; BTREE_NODE_V1_HEADER_LEN] = [
        0x41, 0x4E, 0x42, 0x54, 0x01, 0x00, 0x01, 0x00, 0x2A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00,
        0x00, 0x10, 0x29, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2B, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0F, 0xDA, 0x64, 0x1E, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    assert_header_golden(&node, &EMPTY_LEAF_HEADER);
    assert_eq!(node.header.node_kind, BTreeNodeKindV1::Leaf);
    assert_eq!(BTreeNodeV1::decode(&encoded).expect("node decodes"), node);
    assert_eq!(
        BTreeNodeV1::decode_for_page_id(&encoded, PageId::new(42)).expect("page id matches"),
        node
    );
}
