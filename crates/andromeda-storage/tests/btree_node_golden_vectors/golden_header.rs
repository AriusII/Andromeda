use crate::support::{PAGE_SIZE, encode_node, leaf_node};
use andromeda_storage::{
    BTREE_NODE_V1_FORMAT_VERSION, BTREE_NODE_V1_HEADER_LEN, BTREE_NODE_V1_MAGIC, BTreeNodeKindV1,
    BTreeNodeV1, PageId,
};

#[test]
fn empty_leaf_golden_header_vector_is_stable() {
    let node = leaf_node(42, 7, Vec::new(), Some(41), Some(43));
    let encoded = encode_node(&node);

    const EMPTY_LEAF_HEADER: [u8; BTREE_NODE_V1_HEADER_LEN] = [
        0x41, 0x4E, 0x42, 0x54, 0x01, 0x00, 0x01, 0x00, 0x2A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00,
        0x00, 0x10, 0x29, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2B, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0F, 0xDA, 0x64, 0x1E, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    assert_eq!(encoded.len(), usize::from(PAGE_SIZE));
    assert_eq!(&encoded[..BTREE_NODE_V1_HEADER_LEN], &EMPTY_LEAF_HEADER);
    assert_eq!(&encoded[0..4], &BTREE_NODE_V1_MAGIC.to_le_bytes());
    assert_eq!(&encoded[4..6], &BTREE_NODE_V1_FORMAT_VERSION.to_le_bytes());
    assert_eq!(encoded[6], 1);
    assert_eq!(&encoded[8..16], &42u64.to_le_bytes());
    assert_eq!(&encoded[16..24], &7u64.to_le_bytes());
    assert_eq!(&encoded[24..26], &0u16.to_le_bytes());
    assert_eq!(&encoded[26..28], &0u16.to_le_bytes());
    assert_eq!(
        &encoded[28..30],
        &(BTREE_NODE_V1_HEADER_LEN as u16).to_le_bytes()
    );
    assert_eq!(&encoded[30..32], &4096u16.to_le_bytes());
    assert_eq!(&encoded[32..40], &41u64.to_le_bytes());
    assert_eq!(&encoded[40..48], &43u64.to_le_bytes());
    assert_ne!(&encoded[52..56], &0u32.to_le_bytes());

    let decoded = BTreeNodeV1::decode(&encoded).expect("golden leaf decodes");
    assert_eq!(decoded, node);
    assert_eq!(decoded.header.node_kind, BTreeNodeKindV1::Leaf);
    assert_eq!(
        BTreeNodeV1::decode_for_page_id(&encoded, PageId::new(42)).expect("page id matches"),
        node
    );
}
