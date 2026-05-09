//! Durable B-Tree promotion contract tests owned by the storage-index crate.
//!
//! Recovery-owned WAL replay coverage lives outside this crate. These tests keep
//! the local invariant that logical in-memory B-Tree operations do not imply
//! page-backed durable mutation promotion.

#![forbid(unsafe_code)]

use andromeda_error::{AndromedaError, AndromedaErrorKind};
use andromeda_storage_index::{
    BTREE_DURABLE_FORMAT_PROMOTED, BTreeConfig, BTreeIndexNode, BTreeKeyFormatIdentity,
    BTreeNodeImpl, BTreeOperationType, InMemoryBTreeIndexEngine, IndexId, KeyV1FormatValidator,
    PageId, RowId,
};

#[test]
fn logical_btree_ops_do_not_satisfy_durable_promotion_contract() {
    const {
        assert!(
            !BTREE_DURABLE_FORMAT_PROMOTED,
            "durable B-Tree promotion cannot be claimed by in-memory logical operations"
        );
    }

    let mut engine = default_engine();
    for key in [40u8, 10, 30, 20, 50] {
        engine
            .insert(&[key], RowId::new(u64::from(key) * 10))
            .expect("logical in-memory insert should remain usable for contract fixtures");
    }
    engine
        .delete(&[30])
        .expect("logical in-memory delete should remain usable for contract fixtures");

    assert_eq!(engine.search(&[30]).expect("deleted key lookup"), None);
    assert_eq!(
        engine
            .range_scan(&[10], &[60])
            .expect("ordered logical range scan")
            .into_iter()
            .map(RowId::get)
            .collect::<Vec<_>>(),
        vec![100, 200, 400, 500],
    );

    let config = BTreeConfig::default();
    let mut full_leaf = leaf_node(900, Some(899));
    fill_leaf_to_capacity(&mut full_leaf, &config);

    let (promoted_key, right_leaf) = full_leaf
        .split(config.branching_factor)
        .expect("in-memory fixture split should exercise logical split invariants");

    assert!(!promoted_key.is_empty());
    assert_leaf_keys_strictly_ordered(&full_leaf);
    assert_leaf_keys_strictly_ordered(&right_leaf);
    assert_eq!(full_leaf.next_sibling_page_id, Some(right_leaf.page_id));

    let validator = KeyV1FormatValidator::new(1, 0, BTreeKeyFormatIdentity::V1_0);
    for operation in durable_btree_operations() {
        let error = validator
            .validate_operation(operation)
            .expect_err("durable page-backed B-Tree mutation must remain gated");
        assert_eq!(error.kind(), AndromedaErrorKind::Storage);
        assert!(
            error.message().contains("not promoted") && error.message().contains(operation.name()),
            "operation gate must identify the missing durable mutation contract: {}",
            error.message()
        );
    }

    let mut page_backed_node = BTreeIndexNode::new_leaf(PageId::new(910), PageId::new(900));
    let split_error = page_backed_node
        .split(config.branching_factor)
        .expect_err("page-backed split must fail closed before durable promotion");
    assert_durable_page_gate(&split_error, "page-backed node split");

    let sibling = BTreeIndexNode::new_leaf(PageId::new(911), PageId::new(900));
    let merge_error = page_backed_node
        .merge(&sibling)
        .expect_err("page-backed merge must fail closed before durable promotion");
    assert_durable_page_gate(&merge_error, "page-backed node merge");
}

fn durable_btree_operations() -> [BTreeOperationType; 4] {
    [
        BTreeOperationType::Insert,
        BTreeOperationType::Delete,
        BTreeOperationType::Split,
        BTreeOperationType::Merge,
    ]
}

fn assert_durable_page_gate(error: &AndromedaError, operation: &'static str) {
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(
        error.message().contains(operation)
            && error.message().contains("not promoted")
            && error.message().contains("WAL payload decoding")
            && error.message().contains("idempotent recovery"),
        "page-backed durable gate should name the missing promotion work: {}",
        error.message()
    );
}

fn default_engine() -> InMemoryBTreeIndexEngine {
    InMemoryBTreeIndexEngine::new(IndexId::new(1), PageId::new(10), BTreeConfig::default())
}

fn leaf_node(page_id: u64, parent_page_id: Option<u64>) -> BTreeNodeImpl {
    BTreeNodeImpl::new_leaf(PageId::new(page_id), parent_page_id.map(PageId::new))
}

fn fill_leaf_to_capacity(node: &mut BTreeNodeImpl, config: &BTreeConfig) {
    for key in 0..(config.branching_factor - 1) {
        node.insert_into_leaf(vec![key as u8], RowId::new(key as u64))
            .expect("capacity fixture inserts unique keys");
    }
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
