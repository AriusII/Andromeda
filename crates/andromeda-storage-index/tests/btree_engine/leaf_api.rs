use andromeda_error::AndromedaErrorKind;
use andromeda_storage_index::RowId;

use crate::support::{assert_leaf_keys_strictly_ordered, insert_leaf_keys, leaf_node};

#[test]
fn test_insert_into_leaf_single() {
    let mut node = leaf_node(1, None);
    let row_id = RowId::new(42);
    let key = vec![5, 4, 3];

    node.insert_into_leaf(key.clone(), row_id).unwrap();

    assert_eq!(node.key_value_pairs.len(), 1);
    assert_eq!(node.key_value_pairs[0].key, key);
}

#[test]
fn test_insert_into_leaf_multiple_ordered() {
    let mut node = leaf_node(1, None);

    let keys = vec![
        (vec![10], RowId::new(100)),
        (vec![20], RowId::new(200)),
        (vec![15], RowId::new(150)),
        (vec![5], RowId::new(50)),
        (vec![30], RowId::new(300)),
    ];

    for (key, row_id) in keys {
        node.insert_into_leaf(key, row_id).unwrap();
    }

    assert_eq!(node.key_value_pairs.len(), 5);
    assert_leaf_keys_strictly_ordered(&node);
    assert_eq!(
        node.key_value_pairs
            .iter()
            .map(|kvp| kvp.key.clone())
            .collect::<Vec<_>>(),
        vec![vec![5], vec![10], vec![15], vec![20], vec![30]]
    );
}

#[test]
fn test_insert_into_leaf_duplicate_key_error() {
    let mut node = leaf_node(1, None);
    let key = vec![100];
    let row_id1 = RowId::new(1);
    let row_id2 = RowId::new(2);

    node.insert_into_leaf(key.clone(), row_id1).unwrap();
    let result = node.insert_into_leaf(key, row_id2);

    let error = result.expect_err("duplicate leaf key should fail");
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(error.message().contains("duplicate key"));
    assert_eq!(node.key_value_pairs.len(), 1);
    assert_eq!(node.lookup_in_leaf(&[100]), Some(row_id1));
}

#[test]
fn test_lookup_in_leaf_found() {
    let mut node = leaf_node(1, None);
    let key = vec![42, 43, 44];
    let row_id = RowId::new(999);

    node.insert_into_leaf(key.clone(), row_id).unwrap();
    let found = node.lookup_in_leaf(&key);

    assert_eq!(found, Some(row_id));
}

#[test]
fn test_lookup_in_leaf_not_found() {
    let mut node = leaf_node(1, None);
    let key1 = vec![10];
    let row_id = RowId::new(100);

    node.insert_into_leaf(key1, row_id).unwrap();

    let key2 = vec![20];
    let found = node.lookup_in_leaf(&key2);

    assert_eq!(found, None);
}

#[test]
fn test_delete_from_leaf_success() {
    let mut node = leaf_node(1, None);
    let key = vec![77];
    let row_id = RowId::new(777);

    node.insert_into_leaf(key.clone(), row_id).unwrap();
    assert_eq!(node.key_value_pairs.len(), 1);

    node.delete_from_leaf(&key).unwrap();
    assert_eq!(node.key_value_pairs.len(), 0);
}

#[test]
fn test_delete_from_leaf_key_not_found() {
    let mut node = leaf_node(1, None);
    let key_not_in_node = vec![99];

    let result = node.delete_from_leaf(&key_not_in_node);
    let error = result.expect_err("missing key delete should fail");
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(error.message().contains("key not found"));
    assert!(node.key_value_pairs.is_empty());
}

#[test]
fn test_delete_from_leaf_multiple() {
    let mut node = leaf_node(1, None);

    insert_leaf_keys(&mut node, [10, 20, 30]);
    assert_eq!(node.key_value_pairs.len(), 3);

    node.delete_from_leaf(&[20]).unwrap();
    assert_eq!(node.key_value_pairs.len(), 2);
    assert_eq!(node.key_value_pairs[0].key, vec![10]);
    assert_eq!(node.key_value_pairs[1].key, vec![30]);
}
