use andromeda_storage_index::{BTreeConfig, KeyValuePair, PageId};

use crate::support::{empty_value_pair, fill_leaf_to_capacity, internal_node, leaf_node};

#[test]
fn test_btree_node_creation_leaf() {
    let page_id = PageId::new(100);
    let node = leaf_node(100, None);

    assert!(node.is_leaf);
    assert_eq!(node.page_id, page_id);
    assert_eq!(node.key_value_pairs.len(), 0);
    assert_eq!(node.child_page_ids.len(), 0);
    assert_eq!(node.next_sibling_page_id, None);
}

#[test]
fn test_btree_node_creation_internal() {
    let page_id = PageId::new(200);
    let parent_id = PageId::new(199);
    let node = internal_node(200, Some(199));

    assert!(!node.is_leaf);
    assert_eq!(node.page_id, page_id);
    assert_eq!(node.parent_page_id, Some(parent_id));
    assert_eq!(node.key_value_pairs.len(), 0);
}

#[test]
fn test_node_occupancy_empty() {
    let node = leaf_node(1, None);
    let config = BTreeConfig::default();

    assert!(!node.is_full(config.branching_factor));
    assert!(node.is_underfull(config.branching_factor));
}

#[test]
fn test_node_occupancy_full() {
    let mut node = leaf_node(1, Some(0));
    let config = BTreeConfig::default();

    fill_leaf_to_capacity(&mut node, &config);

    assert!(node.is_full(config.branching_factor));
    assert!(!node.is_underfull(config.branching_factor));
}

#[test]
fn test_node_find_key_index_exact() {
    let mut node = leaf_node(1, None);
    node.key_value_pairs.extend([
        empty_value_pair(10),
        empty_value_pair(30),
        empty_value_pair(50),
    ]);

    assert_eq!(node.find_key_index(&[10]), 0);
    assert_eq!(node.find_key_index(&[30]), 1);
    assert_eq!(node.find_key_index(&[50]), 2);
}

#[test]
fn test_node_find_key_index_insertion() {
    let mut node = leaf_node(1, None);
    node.key_value_pairs.extend([
        KeyValuePair {
            key: vec![10],
            value: vec![],
        },
        KeyValuePair {
            key: vec![50],
            value: vec![],
        },
    ]);

    assert_eq!(node.find_key_index(&[5]), 0);
    assert_eq!(node.find_key_index(&[20]), 1);
    assert_eq!(node.find_key_index(&[60]), 2);
}

#[test]
fn test_internal_node_find_child_index() {
    let mut node = internal_node(1, None);

    node.key_value_pairs
        .extend([empty_value_pair(50), empty_value_pair(100)]);
    node.child_page_ids = vec![PageId::new(1), PageId::new(2), PageId::new(3)];

    assert_eq!(node.find_child_index(&[25]), 0);
    assert_eq!(node.find_child_index(&[50]), 1);
    assert_eq!(node.find_child_index(&[75]), 1);
    assert_eq!(node.find_child_index(&[100]), 2);
    assert_eq!(node.find_child_index(&[150]), 2);
}

#[test]
fn test_internal_node_get_child_page_id() {
    let mut node = internal_node(1, None);
    node.child_page_ids
        .extend([PageId::new(10), PageId::new(20), PageId::new(30)]);

    assert_eq!(node.get_child_page_id(0), Some(PageId::new(10)));
    assert_eq!(node.get_child_page_id(1), Some(PageId::new(20)));
    assert_eq!(node.get_child_page_id(2), Some(PageId::new(30)));
    assert_eq!(node.get_child_page_id(3), None);
}
