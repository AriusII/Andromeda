use crate::support::{PAGE_SIZE, encode_node, leaf_entry};
use andromeda_storage as storage;
use andromeda_storage_page as page;

#[test]
fn storage_facade_encoded_leaf_roundtrips_through_owner_decoder() {
    let node = storage::BTreeNodeV1::new_leaf(
        storage::PageId::new(50),
        storage::Lsn::new(8),
        vec![
            leaf_entry(b"a", 10),
            leaf_entry(b"b", 11),
            leaf_entry(b"c", 12),
        ],
        None,
        Some(storage::PageId::new(51)),
        PAGE_SIZE,
    )
    .expect("facade leaf constructor stays available");

    let encoded = encode_node(&node);
    let decoded = page::BTreeNodeV1::decode(&encoded).expect("owner decodes facade leaf image");
    assert_eq!(
        decoded.keys,
        vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]
    );
    assert_eq!(decoded.leaf_values.len(), 3);
    let high_key_offset = decoded
        .header
        .high_key_offset
        .expect("multi-key leaf carries high-key offset");
    assert!(high_key_offset >= storage::BTREE_NODE_V1_HEADER_LEN as u16);
    assert!(high_key_offset < decoded.header.free_start);
    assert_eq!(decoded.key_count(), 3);
    assert_eq!(decoded.encoded_len(), decoded.header.free_start as usize);
}

#[test]
fn storage_facade_encoded_internal_roundtrips_through_owner_decoder() {
    let node = storage::BTreeNodeV1::new_internal(
        storage::PageId::new(70),
        storage::Lsn::new(9),
        vec![b"k10".to_vec(), b"k20".to_vec()],
        vec![
            storage::PageId::new(100),
            storage::PageId::new(101),
            storage::PageId::new(102),
        ],
        PAGE_SIZE,
    )
    .expect("facade internal constructor stays available");

    let encoded = encode_node(&node);
    assert_eq!(encoded[6], 2);
    assert_eq!(&encoded[26..28], &3u16.to_le_bytes());
    assert_eq!(
        &encoded[storage::BTREE_NODE_V1_HEADER_LEN..storage::BTREE_NODE_V1_HEADER_LEN + 8],
        &100u64.to_le_bytes()
    );

    let decoded = page::BTreeNodeV1::decode(&encoded).expect("owner decodes facade internal image");
    assert_eq!(decoded.header.node_kind, page::BTreeNodeKindV1::Internal);
    assert_eq!(
        decoded.child_page_ids,
        vec![
            page::PageId::new(100),
            page::PageId::new(101),
            page::PageId::new(102)
        ]
    );
    assert_eq!(decoded.keys, vec![b"k10".to_vec(), b"k20".to_vec()]);
}
