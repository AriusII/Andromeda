use andromeda_storage_index::{BTreeNodeImpl, KeyValuePair, PageId, RowId};

use crate::support::{assert_leaf_keys_strictly_ordered, row_id_value, split_test_config};

#[test]
fn leaf_split_preserves_all_keys_order_and_sibling_chain() {
    let config = split_test_config();
    let mut left = BTreeNodeImpl::new_leaf(PageId::new(10), Some(PageId::new(1)));
    left.next_sibling_page_id = Some(PageId::new(99));

    for key in [60u8, 10, 70, 20, 50, 30, 40] {
        left.insert_into_leaf(vec![key], RowId::new(key as u64))
            .unwrap();
    }

    assert!(left.is_full(config.branching_factor));
    let expected_keys = left
        .key_value_pairs
        .iter()
        .map(|kvp| kvp.key.clone())
        .collect::<Vec<_>>();

    let (promoted_key, right) = left.split(config.branching_factor).unwrap();

    assert_leaf_keys_strictly_ordered(&left);
    assert_leaf_keys_strictly_ordered(&right);
    assert_eq!(left.next_sibling_page_id, Some(right.page_id));
    assert_eq!(right.next_sibling_page_id, Some(PageId::new(99)));
    assert_eq!(promoted_key, right.key_value_pairs[0].key);
    assert!(
        left.key_value_pairs.last().unwrap().key < right.key_value_pairs.first().unwrap().key,
        "split leaves must remain globally ordered"
    );

    let recovered_keys = left
        .key_value_pairs
        .iter()
        .chain(right.key_value_pairs.iter())
        .map(|kvp| kvp.key.clone())
        .collect::<Vec<_>>();
    assert_eq!(recovered_keys, expected_keys);

    let recovered_row_ids = left
        .key_value_pairs
        .iter()
        .chain(right.key_value_pairs.iter())
        .map(row_id_value)
        .collect::<Vec<_>>();
    assert_eq!(recovered_row_ids, vec![10, 20, 30, 40, 50, 60, 70]);
}

#[test]
fn internal_split_preserves_child_count_and_separator_order() {
    let config = split_test_config();
    let mut node = BTreeNodeImpl::new_internal(PageId::new(20), Some(PageId::new(1)));

    for key in [10u8, 20, 30, 40, 50, 60, 70] {
        node.key_value_pairs.push(KeyValuePair {
            key: vec![key],
            value: Vec::new(),
        });
    }
    for child in 0..8 {
        node.child_page_ids.push(PageId::new(100 + child));
    }

    let (promoted_key, right) = node.split(config.branching_factor).unwrap();

    assert_eq!(promoted_key, vec![40]);
    assert_eq!(node.key_value_pairs.len() + 1, node.child_page_ids.len());
    assert_eq!(right.key_value_pairs.len() + 1, right.child_page_ids.len());
    assert!(
        node.key_value_pairs.last().unwrap().key < promoted_key
            && promoted_key < right.key_value_pairs.first().unwrap().key
    );
}
