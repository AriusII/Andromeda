use crate::support::{encode_node, internal_node, leaf_entry, leaf_node};
use andromeda_storage::{BTREE_NODE_V1_HEADER_LEN, BTreeNodeKindV1, BTreeNodeV1, PageId};

#[test]
fn leaf_with_multiple_keys_roundtrips_and_high_key_is_inside_body() {
    let node = leaf_node(
        50,
        8,
        vec![
            leaf_entry(b"a", 10),
            leaf_entry(b"b", 11),
            leaf_entry(b"c", 12),
        ],
        None,
        Some(51),
    );

    let encoded = encode_node(&node);
    let decoded = BTreeNodeV1::decode(&encoded).expect("leaf decodes");
    assert_eq!(
        decoded.keys,
        vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]
    );
    assert_eq!(decoded.leaf_values.len(), 3);
    let high_key_offset = decoded
        .header
        .high_key_offset
        .expect("multi-key leaf carries high-key offset");
    assert!(high_key_offset >= BTREE_NODE_V1_HEADER_LEN as u16);
    assert!(high_key_offset < decoded.header.free_start);
    assert_eq!(decoded.key_count(), 3);
    assert_eq!(decoded.encoded_len(), decoded.header.free_start as usize);
}

#[test]
fn internal_node_roundtrips_children_before_keys() {
    let node = internal_node(
        70,
        9,
        vec![b"k10".to_vec(), b"k20".to_vec()],
        vec![100, 101, 102],
    );

    let encoded = encode_node(&node);
    assert_eq!(encoded[6], 2);
    assert_eq!(&encoded[26..28], &3u16.to_le_bytes());
    assert_eq!(
        &encoded[BTREE_NODE_V1_HEADER_LEN..BTREE_NODE_V1_HEADER_LEN + 8],
        &100u64.to_le_bytes()
    );

    let decoded = BTreeNodeV1::decode(&encoded).expect("internal image decodes");
    assert_eq!(decoded.header.node_kind, BTreeNodeKindV1::Internal);
    assert_eq!(
        decoded.child_page_ids,
        vec![PageId::new(100), PageId::new(101), PageId::new(102)]
    );
    assert_eq!(decoded.keys, vec![b"k10".to_vec(), b"k20".to_vec()]);
}

#[test]
fn leaf_sibling_links_reencode_stably() {
    let node = leaf_node(
        99,
        20,
        vec![leaf_entry(b"a", 10), leaf_entry(b"z", 11)],
        Some(98),
        Some(100),
    );

    let encoded = encode_node(&node);
    let decoded = BTreeNodeV1::decode(&encoded).expect("leaf decodes");
    assert_eq!(decoded.header.prev_leaf, Some(PageId::new(98)));
    assert_eq!(decoded.header.next_leaf, Some(PageId::new(100)));
    assert_eq!(decoded.key_count(), 2);
    assert_eq!(decoded.encode().expect("leaf re-encodes"), encoded);
}

#[test]
fn internal_children_reencode_stably() {
    let node = internal_node(
        120,
        21,
        vec![b"k10".to_vec(), b"k20".to_vec(), b"k30".to_vec()],
        vec![200, 201, 202, 203],
    );

    let encoded = encode_node(&node);
    let decoded = BTreeNodeV1::decode(&encoded).expect("internal image decodes");
    assert_eq!(
        decoded.child_page_ids,
        vec![
            PageId::new(200),
            PageId::new(201),
            PageId::new(202),
            PageId::new(203)
        ]
    );
    assert_eq!(decoded.key_count(), 3);
    assert_eq!(decoded.encode().expect("internal re-encodes"), encoded);
}
