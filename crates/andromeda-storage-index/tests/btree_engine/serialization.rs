use andromeda_storage_index::{KeyValuePair, PageId, RowId};

use crate::support::{internal_node, leaf_node};

#[test]
fn test_serialize_deserialize_empty_leaf() {
    let node = leaf_node(42, None);
    let serialized = node.serialize().expect("serialize BTreeNodeImpl");
    let deserialized =
        andromeda_storage_index::BTreeNodeImpl::deserialize(PageId::new(42), &serialized).unwrap();

    assert!(deserialized.is_leaf);
    assert_eq!(deserialized.page_id, PageId::new(42));
    assert_eq!(deserialized.key_value_pairs.len(), 0);
}

#[test]
fn test_serialize_deserialize_leaf_with_data() {
    let mut node = leaf_node(1, Some(0));
    let row_id = RowId::new(42);
    let key = vec![10, 20, 30];

    node.insert_into_leaf(key.clone(), row_id).unwrap();

    let serialized = node.serialize().expect("serialize BTreeNodeImpl");
    let deserialized =
        andromeda_storage_index::BTreeNodeImpl::deserialize(PageId::new(1), &serialized).unwrap();

    assert!(deserialized.is_leaf);
    assert_eq!(deserialized.key_value_pairs.len(), 1);
    assert_eq!(deserialized.key_value_pairs[0].key, key);
}

#[test]
fn test_serialize_deserialize_preserves_sibling_links() {
    let mut node = leaf_node(1, None);
    node.next_sibling_page_id = Some(PageId::new(2));

    let serialized = node.serialize().expect("serialize BTreeNodeImpl");
    let deserialized =
        andromeda_storage_index::BTreeNodeImpl::deserialize(PageId::new(1), &serialized).unwrap();

    assert_eq!(deserialized.next_sibling_page_id, Some(PageId::new(2)));
}

#[test]
fn test_serialize_deserialize_internal_node() {
    let mut node = internal_node(100, Some(99));
    node.key_value_pairs.push(KeyValuePair {
        key: vec![50],
        value: vec![],
    });
    node.child_page_ids.push(PageId::new(1));
    node.child_page_ids.push(PageId::new(2));

    let serialized = node.serialize().expect("serialize BTreeNodeImpl");
    let deserialized =
        andromeda_storage_index::BTreeNodeImpl::deserialize(PageId::new(100), &serialized).unwrap();

    assert!(!deserialized.is_leaf);
    assert_eq!(deserialized.key_value_pairs.len(), 1);
    assert_eq!(deserialized.child_page_ids.len(), 2);
}

#[test]
fn test_serialize_deserialize_roundtrip_complex() {
    let mut node = leaf_node(1, Some(0));

    for i in 0..10 {
        let key = vec![i as u8];
        let row_id = RowId::new((i as u64) * 100);
        node.insert_into_leaf(key, row_id).unwrap();
    }

    let serialized1 = node.serialize().expect("serialize BTreeNodeImpl");
    let deserialized1 =
        andromeda_storage_index::BTreeNodeImpl::deserialize(PageId::new(1), &serialized1).unwrap();
    let serialized2 = deserialized1
        .serialize()
        .expect("serialize deserialized BTreeNodeImpl");

    assert_eq!(serialized1, serialized2);
}

#[test]
fn malformed_node_bytes_are_rejected_without_defaulting_fields() {
    let page_id = PageId::new(77);
    let mut serialized = leaf_node(77, None)
        .serialize()
        .expect("serialize BTreeNodeImpl");

    serialized[0] = 9;
    let invalid_tag =
        andromeda_storage_index::BTreeNodeImpl::deserialize(page_id, &serialized).unwrap_err();
    assert!(invalid_tag.message().contains("invalid node format"));

    let truncated_header =
        andromeda_storage_index::BTreeNodeImpl::deserialize(page_id, &serialized[..20])
            .unwrap_err();
    assert!(truncated_header.message().contains("invalid node format"));

    let mut truncated_key = leaf_node(77, None)
        .serialize()
        .expect("serialize BTreeNodeImpl");
    truncated_key[1..3].copy_from_slice(&1u16.to_le_bytes());
    let missing_key_bytes =
        andromeda_storage_index::BTreeNodeImpl::deserialize(page_id, &truncated_key).unwrap_err();
    assert!(missing_key_bytes.message().contains("invalid node format"));
}

#[test]
fn serialize_rejects_key_count_that_exceeds_node_format_limit() {
    let mut node = leaf_node(88, None);
    node.key_value_pairs = vec![
        KeyValuePair {
            key: Vec::new(),
            value: Vec::new(),
        };
        u16::MAX as usize + 1
    ];

    let error = node.serialize().unwrap_err();

    assert!(error.message().contains("key_count"));
    assert!(error.message().contains("exceeds max"));
}

#[test]
fn serialize_rejects_key_payload_that_exceeds_node_format_limit() {
    let mut node = leaf_node(89, None);
    node.key_value_pairs.push(KeyValuePair {
        key: vec![0; u16::MAX as usize + 1],
        value: Vec::new(),
    });

    let error = node.serialize().unwrap_err();

    assert!(error.message().contains("key_len"));
    assert!(error.message().contains("exceeds max"));
}
