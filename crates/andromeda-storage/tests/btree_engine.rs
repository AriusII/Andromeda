//! Comprehensive tests for the B+ Tree Index Engine
//!
//! Tests cover:
//! - Basic insert/search/delete operations
//! - Node splitting and merging
//! - Range scan operations
//! - Leaf node linking
//! - Serialization/deserialization roundtrips
//! - Occupancy invariants
//! - Error conditions

#[cfg(test)]
mod btree_engine_tests {
    use andromeda_core::AndromedaErrorKind;
    use andromeda_storage::{
        BTREE_DURABLE_FORMAT_PROMOTED, BTreeConfig, BTreeNodeImpl, InMemoryBTreeIndexEngine,
        IndexId, KeyValuePair, PageId, RowId,
    };

    #[test]
    fn test_btree_node_creation_leaf() {
        let page_id = PageId::new(100);
        let node = BTreeNodeImpl::new_leaf(page_id, None);

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
        let node = BTreeNodeImpl::new_internal(page_id, Some(parent_id));

        assert!(!node.is_leaf);
        assert_eq!(node.page_id, page_id);
        assert_eq!(node.parent_page_id, Some(parent_id));
        assert_eq!(node.key_value_pairs.len(), 0);
    }

    #[test]
    fn test_node_occupancy_empty() {
        let node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        let config = BTreeConfig::default();

        assert!(!node.is_full(config.branching_factor));
        assert!(node.is_underfull(config.branching_factor));
    }

    #[test]
    fn test_node_occupancy_full() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), Some(PageId::new(0)));
        let config = BTreeConfig::default();

        // Fill to capacity
        for i in 0..(config.branching_factor - 1) {
            node.key_value_pairs.push(KeyValuePair {
                key: vec![i as u8],
                value: vec![],
            });
        }

        assert!(node.is_full(config.branching_factor));
        assert!(!node.is_underfull(config.branching_factor));
    }

    #[test]
    fn test_node_find_key_index_exact() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        node.key_value_pairs.push(KeyValuePair {
            key: vec![10],
            value: vec![],
        });
        node.key_value_pairs.push(KeyValuePair {
            key: vec![30],
            value: vec![],
        });
        node.key_value_pairs.push(KeyValuePair {
            key: vec![50],
            value: vec![],
        });

        assert_eq!(node.find_key_index(&[10]), 0);
        assert_eq!(node.find_key_index(&[30]), 1);
        assert_eq!(node.find_key_index(&[50]), 2);
    }

    #[test]
    fn test_node_find_key_index_insertion() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        node.key_value_pairs.push(KeyValuePair {
            key: vec![10],
            value: vec![],
        });
        node.key_value_pairs.push(KeyValuePair {
            key: vec![50],
            value: vec![],
        });

        assert_eq!(node.find_key_index(&[5]), 0);
        assert_eq!(node.find_key_index(&[20]), 1);
        assert_eq!(node.find_key_index(&[60]), 2);
    }

    #[test]
    fn test_insert_into_leaf_single() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        let row_id = RowId::new(42);
        let key = vec![5, 4, 3];

        node.insert_into_leaf(key.clone(), row_id).unwrap();

        assert_eq!(node.key_value_pairs.len(), 1);
        assert_eq!(node.key_value_pairs[0].key, key);
    }

    #[test]
    fn test_insert_into_leaf_multiple_ordered() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);

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

        // Verify sorted order
        assert_eq!(node.key_value_pairs[0].key, vec![5]);
        assert_eq!(node.key_value_pairs[1].key, vec![10]);
        assert_eq!(node.key_value_pairs[2].key, vec![15]);
        assert_eq!(node.key_value_pairs[3].key, vec![20]);
        assert_eq!(node.key_value_pairs[4].key, vec![30]);
    }

    #[test]
    fn test_insert_into_leaf_duplicate_key_error() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
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
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        let key = vec![42, 43, 44];
        let row_id = RowId::new(999);

        node.insert_into_leaf(key.clone(), row_id).unwrap();
        let found = node.lookup_in_leaf(&key);

        assert_eq!(found, Some(row_id));
    }

    #[test]
    fn test_lookup_in_leaf_not_found() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        let key1 = vec![10];
        let row_id = RowId::new(100);

        node.insert_into_leaf(key1, row_id).unwrap();

        let key2 = vec![20];
        let found = node.lookup_in_leaf(&key2);

        assert_eq!(found, None);
    }

    #[test]
    fn test_delete_from_leaf_success() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        let key = vec![77];
        let row_id = RowId::new(777);

        node.insert_into_leaf(key.clone(), row_id).unwrap();
        assert_eq!(node.key_value_pairs.len(), 1);

        node.delete_from_leaf(&key).unwrap();
        assert_eq!(node.key_value_pairs.len(), 0);
    }

    #[test]
    fn test_delete_from_leaf_key_not_found() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        let key_not_in_node = vec![99];

        let result = node.delete_from_leaf(&key_not_in_node);
        let error = result.expect_err("missing key delete should fail");
        assert_eq!(error.kind(), AndromedaErrorKind::Storage);
        assert!(error.message().contains("key not found"));
        assert!(node.key_value_pairs.is_empty());
    }

    #[test]
    fn test_delete_from_leaf_multiple() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);

        let keys = [vec![10], vec![20], vec![30]];
        for (i, key) in keys.iter().enumerate() {
            node.insert_into_leaf(key.clone(), RowId::new(i as u64))
                .unwrap();
        }

        assert_eq!(node.key_value_pairs.len(), 3);

        node.delete_from_leaf(&[20]).unwrap();
        assert_eq!(node.key_value_pairs.len(), 2);

        assert_eq!(node.key_value_pairs[0].key, vec![10]);
        assert_eq!(node.key_value_pairs[1].key, vec![30]);
    }

    #[test]
    fn test_split_leaf_node() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), Some(PageId::new(0)));
        let config = BTreeConfig::default();

        // Fill node to capacity
        for i in 0..(config.branching_factor - 1) {
            let key = vec![i as u8];
            let row_id = RowId::new(i as u64);
            node.insert_into_leaf(key, row_id).ok();
        }

        assert!(node.is_full(config.branching_factor));

        let (promoted_key, new_node) = node.split(config.branching_factor).unwrap();

        assert!(!promoted_key.is_empty());
        assert!(!node.key_value_pairs.is_empty());
        assert!(!new_node.key_value_pairs.is_empty());

        // Verify leaf sibling linking
        assert_eq!(node.next_sibling_page_id, Some(new_node.page_id));
        assert_eq!(new_node.next_sibling_page_id, None);
    }

    #[test]
    fn test_split_leaf_preserves_data() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), Some(PageId::new(0)));
        let config = BTreeConfig::default();

        let mut all_kvps = Vec::new();
        for i in 0..(config.branching_factor - 1) {
            let key = vec![i as u8];
            let row_id = RowId::new(i as u64 * 10);
            node.insert_into_leaf(key.clone(), row_id).ok();
            all_kvps.push((key, row_id));
        }

        let (_, new_node) = node.split(config.branching_factor).unwrap();

        let mut recovered_kvps = Vec::new();
        for kvp in &node.key_value_pairs {
            if let Ok(id_bytes) = <[u8; 8]>::try_from(kvp.value.as_slice()) {
                recovered_kvps.push((kvp.key.clone(), RowId::new(u64::from_le_bytes(id_bytes))));
            }
        }
        for kvp in &new_node.key_value_pairs {
            if let Ok(id_bytes) = <[u8; 8]>::try_from(kvp.value.as_slice()) {
                recovered_kvps.push((kvp.key.clone(), RowId::new(u64::from_le_bytes(id_bytes))));
            }
        }

        assert_eq!(recovered_kvps.len(), all_kvps.len());
    }

    #[test]
    fn test_split_internal_node() {
        let mut node = BTreeNodeImpl::new_internal(PageId::new(1), Some(PageId::new(0)));
        let config = BTreeConfig::default();

        // Build internal node with keys and child pointers
        for i in 0..(config.branching_factor - 1) {
            node.key_value_pairs.push(KeyValuePair {
                key: vec![i as u8],
                value: vec![],
            });
        }
        for i in 0..config.branching_factor {
            node.child_page_ids.push(PageId::new((i as u64) * 100));
        }

        let (promoted_key, new_node) = node.split(config.branching_factor).unwrap();

        assert!(!promoted_key.is_empty());
        assert!(!node.child_page_ids.is_empty());
        assert!(!new_node.child_page_ids.is_empty());

        // Verify invariant: internal node has n+1 children for n keys
        assert_eq!(node.key_value_pairs.len() + 1, node.child_page_ids.len());
        assert_eq!(
            new_node.key_value_pairs.len() + 1,
            new_node.child_page_ids.len()
        );
    }

    #[test]
    fn test_serialize_deserialize_empty_leaf() {
        let node = BTreeNodeImpl::new_leaf(PageId::new(42), None);
        let serialized = node.serialize();
        let deserialized = BTreeNodeImpl::deserialize(PageId::new(42), &serialized).unwrap();

        assert!(deserialized.is_leaf);
        assert_eq!(deserialized.page_id, PageId::new(42));
        assert_eq!(deserialized.key_value_pairs.len(), 0);
    }

    #[test]
    fn test_serialize_deserialize_leaf_with_data() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), Some(PageId::new(0)));
        let row_id = RowId::new(42);
        let key = vec![10, 20, 30];

        node.insert_into_leaf(key.clone(), row_id).unwrap();

        let serialized = node.serialize();
        let deserialized = BTreeNodeImpl::deserialize(PageId::new(1), &serialized).unwrap();

        assert!(deserialized.is_leaf);
        assert_eq!(deserialized.key_value_pairs.len(), 1);
        assert_eq!(deserialized.key_value_pairs[0].key, key);
    }

    #[test]
    fn test_serialize_deserialize_preserves_sibling_links() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        node.next_sibling_page_id = Some(PageId::new(2));

        let serialized = node.serialize();
        let deserialized = BTreeNodeImpl::deserialize(PageId::new(1), &serialized).unwrap();

        assert_eq!(deserialized.next_sibling_page_id, Some(PageId::new(2)));
    }

    #[test]
    fn test_serialize_deserialize_internal_node() {
        let mut node = BTreeNodeImpl::new_internal(PageId::new(100), Some(PageId::new(99)));
        node.key_value_pairs.push(KeyValuePair {
            key: vec![50],
            value: vec![],
        });
        node.child_page_ids.push(PageId::new(1));
        node.child_page_ids.push(PageId::new(2));

        let serialized = node.serialize();
        let deserialized = BTreeNodeImpl::deserialize(PageId::new(100), &serialized).unwrap();

        assert!(!deserialized.is_leaf);
        assert_eq!(deserialized.key_value_pairs.len(), 1);
        assert_eq!(deserialized.child_page_ids.len(), 2);
    }

    #[test]
    fn test_serialize_deserialize_roundtrip_complex() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), Some(PageId::new(0)));

        for i in 0..10 {
            let key = vec![i as u8];
            let row_id = RowId::new((i as u64) * 100);
            node.insert_into_leaf(key, row_id).ok();
        }

        let serialized1 = node.serialize();
        let deserialized1 = BTreeNodeImpl::deserialize(PageId::new(1), &serialized1).unwrap();
        let serialized2 = deserialized1.serialize();

        assert_eq!(serialized1, serialized2);
    }

    #[test]
    fn test_internal_node_find_child_index() {
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

        assert_eq!(node.find_child_index(&[25]), 0);
        assert_eq!(node.find_child_index(&[50]), 1);
        assert_eq!(node.find_child_index(&[75]), 1);
        assert_eq!(node.find_child_index(&[100]), 2);
        assert_eq!(node.find_child_index(&[150]), 2);
    }

    #[test]
    fn test_internal_node_get_child_page_id() {
        let mut node = BTreeNodeImpl::new_internal(PageId::new(1), None);
        node.child_page_ids.push(PageId::new(10));
        node.child_page_ids.push(PageId::new(20));
        node.child_page_ids.push(PageId::new(30));

        assert_eq!(node.get_child_page_id(0), Some(PageId::new(10)));
        assert_eq!(node.get_child_page_id(1), Some(PageId::new(20)));
        assert_eq!(node.get_child_page_id(2), Some(PageId::new(30)));
        assert_eq!(node.get_child_page_id(3), None);
    }

    #[test]
    fn test_btree_engine_creation() {
        let engine =
            InMemoryBTreeIndexEngine::new(IndexId::new(1), PageId::new(10), BTreeConfig::default());

        const _: () = assert!(!BTREE_DURABLE_FORMAT_PROMOTED);
        assert_eq!(engine.row_count(), 0);
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
        let engine =
            InMemoryBTreeIndexEngine::new(IndexId::new(1), PageId::new(10), BTreeConfig::default());

        let result = engine.search(&[42]).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_btree_engine_range_scan_empty() {
        let engine =
            InMemoryBTreeIndexEngine::new(IndexId::new(1), PageId::new(10), BTreeConfig::default());

        let results = engine.range_scan(&[0], &[255]).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_btree_engine_insert() {
        let mut engine =
            InMemoryBTreeIndexEngine::new(IndexId::new(1), PageId::new(10), BTreeConfig::default());

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
        let mut engine =
            InMemoryBTreeIndexEngine::new(IndexId::new(1), PageId::new(10), BTreeConfig::default());

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
        let mut engine =
            InMemoryBTreeIndexEngine::new(IndexId::new(1), PageId::new(10), BTreeConfig::default());

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
        let mut engine =
            InMemoryBTreeIndexEngine::new(IndexId::new(1), PageId::new(10), BTreeConfig::default());

        let oversized_key = vec![0u8; 10000];
        let result = engine.delete(&oversized_key);
        let error = result.expect_err("oversized delete key should fail");
        assert_eq!(error.kind(), AndromedaErrorKind::Storage);
        assert!(error.message().contains("key size"));
    }

    #[test]
    fn test_leaf_node_key_ordering_invariant() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);

        // Insert in random order
        let keys = vec![50, 30, 70, 10, 90, 20, 60, 40, 80];
        for (i, key) in keys.iter().enumerate() {
            node.insert_into_leaf(vec![*key], RowId::new(i as u64)).ok();
        }

        // Verify sorted order
        for i in 1..node.key_value_pairs.len() {
            assert!(
                node.key_value_pairs[i - 1].key < node.key_value_pairs[i].key,
                "Keys not in sorted order"
            );
        }
    }

    #[test]
    fn test_child_pointer_invariant_internal_nodes() {
        let mut node = BTreeNodeImpl::new_internal(PageId::new(1), None);

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
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), Some(PageId::new(0)));
        let config = BTreeConfig::default();
        let max_keys = (config.branching_factor - 1) as usize;

        for i in 0..(max_keys + 5) {
            let key = vec![i as u8];
            node.insert_into_leaf(key, RowId::new(i as u64)).ok();
        }

        // Node should not be allowed to exceed max capacity in real implementation
        // But for this test, we verify the is_full check works correctly
        if node.key_value_pairs.len() >= max_keys {
            assert!(node.is_full(config.branching_factor));
        }
    }
}
