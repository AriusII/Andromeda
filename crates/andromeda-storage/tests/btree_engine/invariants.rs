use andromeda_storage::{BTreeConfig, KeyValuePair, PageId, RowId};

use crate::support::{assert_leaf_keys_strictly_ordered, leaf_node};

#[test]
fn test_leaf_node_key_ordering_invariant() {
    let mut node = leaf_node(1, None);

    for (i, key) in [50, 30, 70, 10, 90, 20, 60, 40, 80].iter().enumerate() {
        node.insert_into_leaf(vec![*key], RowId::new(i as u64))
            .unwrap();
    }

    assert_leaf_keys_strictly_ordered(&node);
}

#[test]
fn test_child_pointer_invariant_internal_nodes() {
    let mut node = andromeda_storage::BTreeNodeImpl::new_internal(PageId::new(1), None);

    for i in 0..5 {
        node.key_value_pairs.push(KeyValuePair {
            key: vec![i * 20],
            value: vec![],
        });
    }

    for i in 0..6 {
        node.child_page_ids.push(PageId::new(100 + i as u64));
    }

    assert_eq!(node.key_value_pairs.len() + 1, node.child_page_ids.len());
}

#[test]
fn test_node_fullness_invariant() {
    let mut node = leaf_node(1, Some(0));
    let config = BTreeConfig::default();
    let max_keys = (config.branching_factor - 1) as usize;

    for i in 0..(max_keys + 5) {
        let key = vec![i as u8];
        node.insert_into_leaf(key, RowId::new(i as u64)).unwrap();
    }

    assert!(node.key_value_pairs.len() >= max_keys);
    assert!(node.is_full(config.branching_factor));
}
