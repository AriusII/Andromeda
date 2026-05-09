use super::*;

#[test]
fn test_node_impl_new_leaf() {
    let page_id = PageId::new(1);
    let node = BTreeNodeImpl::new_leaf(page_id, None);
    assert!(node.is_leaf);
    assert_eq!(node.page_id, page_id);
    assert_eq!(node.key_value_pairs.len(), 0);
}

#[test]
fn test_node_impl_new_internal() {
    let page_id = PageId::new(2);
    let node = BTreeNodeImpl::new_internal(page_id, None);
    assert!(!node.is_leaf);
    assert_eq!(node.page_id, page_id);
}

#[test]
fn test_node_is_full() {
    let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
    let config = BTreeConfig::default();

    for i in 0..(config.branching_factor - 1) {
        node.key_value_pairs.push(KeyValuePair {
            key: vec![i as u8],
            value: vec![],
        });
    }

    assert!(node.is_full(config.branching_factor));
}

#[test]
fn test_find_key_index() {
    let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
    node.key_value_pairs.push(KeyValuePair {
        key: vec![1, 2, 3],
        value: vec![],
    });
    node.key_value_pairs.push(KeyValuePair {
        key: vec![5, 6, 7],
        value: vec![],
    });

    assert_eq!(node.find_key_index(&[1, 2, 3]), 0);
    assert_eq!(node.find_key_index(&[5, 6, 7]), 1);
    assert_eq!(node.find_key_index(&[3, 4, 5]), 1);
    assert_eq!(node.find_key_index(&[8, 9]), 2);
}

#[test]
fn test_insert_into_leaf() {
    let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
    let row_id = RowId::new(100);

    node.insert_into_leaf(vec![42, 43], row_id).unwrap();
    assert_eq!(node.key_value_pairs.len(), 1);
    assert_eq!(node.key_value_pairs[0].key, vec![42, 43]);
}

#[test]
fn test_lookup_in_leaf() {
    let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
    let row_id = RowId::new(42);
    let key = vec![5, 4, 3];

    node.insert_into_leaf(key.clone(), row_id).unwrap();
    let found = node.lookup_in_leaf(&key);
    assert_eq!(found, Some(row_id));
}

#[test]
fn test_serialize_deserialize_leaf() {
    let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
    let row_id = RowId::new(99);
    node.insert_into_leaf(vec![10, 11], row_id).unwrap();

    let serialized = node.serialize().expect("serialize BTreeNodeImpl");
    let deserialized = BTreeNodeImpl::deserialize(PageId::new(1), &serialized).unwrap();

    assert!(deserialized.is_leaf);
    assert_eq!(deserialized.key_value_pairs.len(), 1);
    assert_eq!(deserialized.key_value_pairs[0].key, vec![10, 11]);
}

#[test]
fn test_split_leaf_node() {
    let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
    let config = BTreeConfig::default();

    // Fill node to capacity
    for i in 0..(config.branching_factor - 1) {
        let row_id = RowId::new(i as u64);
        node.insert_into_leaf(vec![i as u8], row_id).ok();
    }

    assert!(node.is_full(config.branching_factor));

    let (promoted_key, new_node) = node.split(config.branching_factor).unwrap();
    assert!(!promoted_key.is_empty());
    assert!(!new_node.key_value_pairs.is_empty());
}

#[test]
fn test_delete_from_leaf() {
    let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
    let row_id = RowId::new(50);
    let key = vec![7, 8, 9];

    node.insert_into_leaf(key.clone(), row_id).unwrap();
    assert_eq!(node.key_value_pairs.len(), 1);

    node.delete_from_leaf(&key).unwrap();
    assert_eq!(node.key_value_pairs.len(), 0);
}

#[test]
fn test_btree_index_creation() {
    let index =
        InMemoryBTreeIndexEngine::new(IndexId::new(1), PageId::new(10), BTreeConfig::default());

    assert_eq!(index.row_count(), 0);
}

#[test]
fn test_statistics_default() {
    let index =
        InMemoryBTreeIndexEngine::new(IndexId::new(1), PageId::new(10), BTreeConfig::default());

    let stats = index.statistics();
    assert_eq!(stats.tree_height, 1);
    assert_eq!(stats.leaf_node_count, 1);
}

#[test]
fn test_insert_multiple_ordered_keys() {
    let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);

    for i in 0..5 {
        let row_id = RowId::new(i as u64 * 10);
        node.insert_into_leaf(vec![i as u8], row_id).unwrap();
    }

    assert_eq!(node.key_value_pairs.len(), 5);
    assert_eq!(node.key_value_pairs[0].key, vec![0]);
    assert_eq!(node.key_value_pairs[4].key, vec![4]);
}

#[test]
fn test_duplicate_key_error() {
    let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
    let row_id1 = RowId::new(10);
    let row_id2 = RowId::new(20);
    let key = vec![42];

    node.insert_into_leaf(key.clone(), row_id1).unwrap();
    let result = node.insert_into_leaf(key.clone(), row_id2);

    assert!(result.is_err());
}

#[test]
fn test_node_sibling_linking() {
    let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
    let sibling_id = PageId::new(2);

    node.next_sibling_page_id = Some(sibling_id);
    assert_eq!(node.next_sibling_page_id, Some(sibling_id));
}

#[test]
fn test_internal_node_child_pointer() {
    let mut node = BTreeNodeImpl::new_internal(PageId::new(1), None);
    node.child_page_ids.push(PageId::new(10));
    node.child_page_ids.push(PageId::new(20));

    assert_eq!(node.get_child_page_id(0), Some(PageId::new(10)));
    assert_eq!(node.get_child_page_id(1), Some(PageId::new(20)));
    assert_eq!(node.get_child_page_id(2), None);
}

#[test]
fn test_find_child_index() {
    let mut node = BTreeNodeImpl::new_internal(PageId::new(1), None);
    node.key_value_pairs.push(KeyValuePair {
        key: vec![50],
        value: vec![],
    });
    node.key_value_pairs.push(KeyValuePair {
        key: vec![100],
        value: vec![],
    });
    node.child_page_ids = vec![PageId::new(1), PageId::new(2), PageId::new(3)];

    assert_eq!(node.find_child_index(&[30]), 0);
    assert_eq!(node.find_child_index(&[50]), 1);
    assert_eq!(node.find_child_index(&[75]), 1);
    assert_eq!(node.find_child_index(&[100]), 2);
    assert_eq!(node.find_child_index(&[150]), 2);
}
