use andromeda_storage_index::{BTreeConfig, RowId};

use crate::support::{
    assert_leaf_keys_strictly_ordered, fill_internal_to_capacity, fill_leaf_to_capacity,
    internal_node, leaf_node, row_id_from_value,
};

#[test]
fn test_split_leaf_node() {
    let mut node = leaf_node(1, Some(0));
    let config = BTreeConfig::default();

    fill_leaf_to_capacity(&mut node, &config);

    assert!(node.is_full(config.branching_factor));

    let (promoted_key, new_node) = node.split(config.branching_factor).unwrap();

    assert!(!promoted_key.is_empty());
    assert!(!node.key_value_pairs.is_empty());
    assert!(!new_node.key_value_pairs.is_empty());
    assert_leaf_keys_strictly_ordered(&node);
    assert_leaf_keys_strictly_ordered(&new_node);

    assert_eq!(node.next_sibling_page_id, Some(new_node.page_id));
    assert_eq!(new_node.next_sibling_page_id, None);
}

#[test]
fn test_split_leaf_preserves_data() {
    let mut node = leaf_node(1, Some(0));
    let config = BTreeConfig::default();

    let mut all_kvps = Vec::new();
    for i in 0..(config.branching_factor - 1) {
        let key = vec![i as u8];
        let row_id = RowId::new(i as u64 * 10);
        node.insert_into_leaf(key.clone(), row_id).unwrap();
        all_kvps.push((key, row_id));
    }

    let (_, new_node) = node.split(config.branching_factor).unwrap();

    let recovered_kvps = node
        .key_value_pairs
        .iter()
        .chain(new_node.key_value_pairs.iter())
        .map(|kvp| (kvp.key.clone(), row_id_from_value(kvp)))
        .collect::<Vec<_>>();

    assert_eq!(recovered_kvps, all_kvps);
}

#[test]
fn test_split_internal_node() {
    let mut node = internal_node(1, Some(0));
    let config = BTreeConfig::default();

    fill_internal_to_capacity(&mut node, &config);

    let (promoted_key, new_node) = node.split(config.branching_factor).unwrap();

    assert!(!promoted_key.is_empty());
    assert!(!node.child_page_ids.is_empty());
    assert!(!new_node.child_page_ids.is_empty());
    assert_eq!(node.key_value_pairs.len() + 1, node.child_page_ids.len());
    assert_eq!(
        new_node.key_value_pairs.len() + 1,
        new_node.child_page_ids.len()
    );
}
