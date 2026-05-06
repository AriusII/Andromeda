#![forbid(unsafe_code)]

use andromeda_storage::{
    BTreeConcurrencyPolicy, BTreeConfig, BTreeLatchLevel, BTreeLatchTarget, BTreeNodeImpl,
    InMemoryBTreeIndexEngine, IndexId, KeyValuePair, PageId, RowId,
};

fn row_id_value(kvp: &KeyValuePair) -> u64 {
    let bytes: [u8; 8] = kvp
        .value
        .as_slice()
        .try_into()
        .expect("BTreeNodeImpl leaf values store RowId as 8 little-endian bytes");
    u64::from_le_bytes(bytes)
}

fn assert_leaf_keys_strictly_ordered(node: &BTreeNodeImpl) {
    assert!(node.is_leaf, "expected a leaf node");
    for pair in node.key_value_pairs.windows(2) {
        assert!(
            pair[0].key < pair[1].key,
            "leaf keys must be strictly increasing: {:?} then {:?}",
            pair[0].key,
            pair[1].key
        );
    }
}

#[test]
fn engine_insert_lookup_delete_and_range_are_ordered_e2e() {
    let mut engine =
        InMemoryBTreeIndexEngine::new(IndexId::new(7), PageId::new(700), BTreeConfig::default());

    for key in [40u8, 10, 30, 20, 50] {
        engine
            .insert(&[key], RowId::new(key as u64 * 10))
            .expect("insert should accept unique in-range keys");
    }

    assert_eq!(engine.row_count(), 5);
    assert_eq!(engine.search(&[10]).unwrap(), Some(RowId::new(100)));
    assert_eq!(engine.search(&[25]).unwrap(), None);
    assert!(
        engine.insert(&[20], RowId::new(999)).is_err(),
        "duplicate keys must not overwrite the existing row locator"
    );

    let scanned: Vec<u64> = engine
        .range_scan(&[15], &[45])
        .unwrap()
        .into_iter()
        .map(RowId::get)
        .collect();
    assert_eq!(scanned, vec![200, 300, 400]);

    engine.delete(&[30]).unwrap();
    assert_eq!(engine.search(&[30]).unwrap(), None);
    assert_eq!(engine.row_count(), 4);
    assert_eq!(engine.statistics().total_key_count, 4);
}

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

#[test]
fn leaf_split_preserves_all_keys_order_and_sibling_chain() {
    let config = BTreeConfig {
        branching_factor: 8,
        ..BTreeConfig::default()
    };
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
    let config = BTreeConfig {
        branching_factor: 8,
        ..BTreeConfig::default()
    };
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

#[test]
fn concurrency_policy_blocks_upward_reacquire_and_left_sibling_order() {
    let policy = BTreeConcurrencyPolicy::default();
    let root = BTreeLatchTarget::new(PageId::new(1), BTreeLatchLevel::Root, 0);
    let leaf = BTreeLatchTarget::new(PageId::new(10), BTreeLatchLevel::Leaf, 1);
    let left_sibling = BTreeLatchTarget::new(PageId::new(9), BTreeLatchLevel::Sibling, 1);
    let right_sibling = BTreeLatchTarget::new(PageId::new(11), BTreeLatchLevel::Sibling, 1);

    assert!(policy.can_acquire_after(&[], root));
    assert!(policy.can_acquire_after(&[root], leaf));
    assert!(!policy.can_acquire_after(&[root, leaf], root));
    assert!(!policy.can_acquire_after(&[root, leaf], left_sibling));
    assert!(policy.can_acquire_after(&[root, leaf], right_sibling));
}
