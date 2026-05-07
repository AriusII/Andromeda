use andromeda_core::AndromedaErrorKind;
use andromeda_storage::{
    BTREE_DURABLE_FORMAT_PROMOTED, BTreeConfig, InMemoryBTreeIndexEngine, IndexId, PageId, RowId,
};

use crate::support::default_engine;

#[test]
fn test_btree_engine_creation() {
    let engine = default_engine();

    const _: () = assert!(!BTREE_DURABLE_FORMAT_PROMOTED);
    assert_eq!(engine.row_count(), 0);
    assert_eq!(engine.index_id(), IndexId::new(1));
    assert_eq!(engine.root_page_id(), PageId::new(10));

    let stats = engine.statistics();
    assert_eq!(stats.tree_height, 1);
    assert_eq!(stats.leaf_node_count, 1);
}

#[test]
#[allow(deprecated)]
fn test_btree_engine_deprecated_alias_remains_compatible() {
    let engine_alias: andromeda_storage::BTreeIndexEngine =
        InMemoryBTreeIndexEngine::new(IndexId::new(1), PageId::new(10), BTreeConfig::default());
    assert_eq!(engine_alias.row_count(), 0);
}

#[test]
fn test_btree_engine_search_empty() {
    let engine = default_engine();

    let result = engine.search(&[42]).unwrap();
    assert_eq!(result, None);
}

#[test]
fn test_btree_engine_range_scan_empty() {
    let engine = default_engine();

    let results = engine.range_scan(&[0], &[255]).unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_btree_engine_insert() {
    let mut engine = default_engine();

    let key = vec![1, 2, 3];
    let row_id = RowId::new(42);

    let result = engine.insert(&key, row_id);
    result.expect("insert should succeed");
    assert_eq!(engine.row_count(), 1);
    assert_eq!(
        engine.search(&key).expect("search inserted key"),
        Some(row_id)
    );
}

#[test]
fn test_btree_engine_insert_oversized_key() {
    let mut engine = default_engine();

    let oversized_key = vec![0u8; 10000];
    let row_id = RowId::new(42);

    let result = engine.insert(&oversized_key, row_id);
    let error = result.expect_err("oversized key should fail");
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(error.message().contains("key size"));
    assert_eq!(engine.row_count(), 0);
}

#[test]
fn test_btree_engine_delete() {
    let mut engine = default_engine();

    let key = vec![5];
    let row_id = RowId::new(50);
    engine.insert(&key, row_id).expect("insert before delete");
    assert_eq!(
        engine.search(&key).expect("search before delete"),
        Some(row_id)
    );

    let result = engine.delete(&key);
    result.expect("delete should succeed");
    assert_eq!(engine.row_count(), 0);
    assert_eq!(engine.search(&key).expect("search deleted key"), None);
}

#[test]
fn test_btree_engine_delete_oversized_key() {
    let mut engine = default_engine();

    let oversized_key = vec![0u8; 10000];
    let result = engine.delete(&oversized_key);
    let error = result.expect_err("oversized delete key should fail");
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(error.message().contains("key size"));
}
