use andromeda_storage::{BTreeNodeImpl, PageId, RowId};

use crate::support::assert_leaf_keys_strictly_ordered;

#[test]
fn leaf_insert_api_preserves_strict_key_order_and_lookup_values() {
    let mut leaf = BTreeNodeImpl::new_leaf(PageId::new(1), None);

    for key in [9u8, 1, 7, 3, 5] {
        leaf.insert_into_leaf(vec![key], RowId::new(key as u64 + 100))
            .unwrap();
    }

    assert_leaf_keys_strictly_ordered(&leaf);
    assert_eq!(
        leaf.key_value_pairs
            .iter()
            .map(|kvp| kvp.key[0])
            .collect::<Vec<_>>(),
        vec![1, 3, 5, 7, 9]
    );
    assert_eq!(leaf.lookup_in_leaf(&[7]), Some(RowId::new(107)));
}
