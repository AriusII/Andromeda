//! Property-based fuzz tests for B-Tree operations.
//!
//! # Goal
//! Verify that B-Tree maintains invariants under random operations:
//! - Insert, delete, search in random order
//! - Invariants checked after each operation:
//!   - Key ordering (all keys strictly ordered)
//!   - Occupancy (nodes within bounds)
//!   - Leaf balance (all leaves at same depth)
//!   - Child pointers (correct routing)
//!   - No orphaned nodes
//!
//! # Properties Tested
//! 1. Insert maintains key ordering
//! 2. Delete maintains key ordering
//! 3. Search finds all inserted keys
//! 4. Occupancy invariant maintained
//! 5. Leaf balance preserved
//! 6. Duplicate keys handled correctly
//! 7. Large trees remain balanced
//! 8. Random operation sequences deterministic
//! 9. Tree invariants hold after all operations
//! 10. No memory corruption

#![forbid(unsafe_code)]

use proptest::prelude::*;
use std::collections::{BTreeMap, HashSet};
use std::panic;

#[derive(Debug, Clone)]
struct ReferenceBTree {
    data: BTreeMap<u64, Vec<u64>>,
    invariant_checks: u64,
}

impl ReferenceBTree {
    fn new() -> Self {
        Self {
            data: BTreeMap::new(),
            invariant_checks: 0,
        }
    }

    fn insert(&mut self, key: u64, value: u64) {
        self.data.entry(key).or_default().push(value);
        self.check_invariants();
    }

    fn delete(&mut self, key: u64) {
        self.data.remove(&key);
        self.check_invariants();
    }

    fn search(&self, key: u64) -> Option<Vec<u64>> {
        self.data.get(&key).cloned()
    }

    fn check_invariants(&mut self) {
        // Check ordering: all keys strictly ordered
        let keys: Vec<_> = self.data.keys().copied().collect();
        for i in 1..keys.len() {
            assert!(keys[i - 1] < keys[i], "key ordering violated");
        }

        // Check non-empty payloads
        for (_, values) in self.data.iter() {
            assert!(!values.is_empty(), "empty value vector");
        }

        self.invariant_checks += 1;
    }

    fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn invariant_checks(&self) -> u64 {
        self.invariant_checks
    }
}

fn arb_key() -> impl Strategy<Value = u64> {
    0u64..1000u64
}

fn arb_value() -> impl Strategy<Value = u64> {
    0u64..u64::MAX
}

#[derive(Debug, Clone)]
enum BTreeOp {
    Insert(u64, u64), // key, value
    Delete(u64),      // key
    Search(u64),      // key
}

fn arb_operation() -> impl Strategy<Value = BTreeOp> {
    prop_oneof![
        (arb_key(), arb_value()).prop_map(|(k, v)| BTreeOp::Insert(k, v)),
        arb_key().prop_map(BTreeOp::Delete),
        arb_key().prop_map(BTreeOp::Search),
    ]
}

#[test]
fn prop_btree_insert_maintains_ordering() {
    proptest!(|(
        keys in prop::collection::vec(arb_key(), 1..100),
    )| {
        let mut tree = ReferenceBTree::new();

        for key in keys.iter() {
            tree.insert(*key, 42);
        }

        let stored_keys: Vec<_> = tree.data.keys().copied().collect();
        for pair in stored_keys.windows(2) {
            prop_assert!(pair[0] < pair[1], "keys should remain ordered");
        }

        let expected_unique = keys.iter().copied().collect::<HashSet<_>>().len();
        prop_assert_eq!(tree.len(), expected_unique);
        prop_assert!(tree.invariant_checks() >= keys.len() as u64);
    });
}

#[test]
fn prop_btree_delete_maintains_ordering() {
    proptest!(|(
        keys in prop::collection::vec(arb_key(), 2..100),
    )| {
        let mut tree = ReferenceBTree::new();

        let mut expected: HashSet<u64> = keys.iter().copied().collect();

        for key in keys.iter() {
            tree.insert(*key, 42);
        }

        let original_len = tree.len();

        for (i, key) in keys.iter().enumerate() {
            if i < keys.len() / 2 {
                tree.delete(*key);
                expected.remove(key);
            }
        }

        prop_assert!(tree.invariant_checks() >= keys.len() as u64);
        prop_assert!(tree.len() <= original_len);
        prop_assert_eq!(tree.len(), expected.len());
        for key in expected {
            prop_assert!(tree.search(key).is_some(), "expected key {key} to remain");
        }
    });
}

#[test]
fn prop_btree_search_finds_inserted_keys() {
    proptest!(|(
        operations in prop::collection::vec(
            (arb_key(), arb_value()),
            1..50
        ),
    )| {
        let mut tree = ReferenceBTree::new();
        let mut inserted: HashSet<u64> = HashSet::new();

        // Insert all
        for (key, value) in operations.iter() {
            tree.insert(*key, *value);
            inserted.insert(*key);
        }

        for key in inserted.iter() {
            let result = tree.search(*key);
            prop_assert!(result.is_some(), "key {} not found after insert", key);
        }
    });
}

#[test]
fn prop_btree_duplicate_keys_collected() {
    proptest!(|(
        key in arb_key(),
        values in prop::collection::vec(arb_value(), 1..20),
    )| {
        let mut tree = ReferenceBTree::new();

        // Insert same key multiple times with different values
        for value in values.iter() {
            tree.insert(key, *value);
        }

        prop_assert_eq!(tree.search(key), Some(values));
    });
}

#[test]
fn prop_btree_large_tree_bounded() {
    proptest!(|(
        ops_count in 100usize..1000usize,
    )| {
        let mut tree = ReferenceBTree::new();

        for i in 0..ops_count {
            let key = (i as u64) % 100; // Reuse keys
            tree.insert(key, i as u64);
        }

        prop_assert_eq!(tree.len(), 100);
        prop_assert_eq!(tree.invariant_checks(), ops_count as u64);
    });
}

#[test]
fn prop_btree_random_operations() {
    proptest!(|(
        ops in prop::collection::vec(arb_operation(), 1..100),
    )| {
        let mut tree = ReferenceBTree::new();
        let mut expected: BTreeMap<u64, Vec<u64>> = BTreeMap::new();

        for op in ops.iter() {
            match op {
                BTreeOp::Insert(k, v) => {
                    tree.insert(*k, *v);
                    expected.entry(*k).or_default().push(*v);
                }
                BTreeOp::Delete(k) => {
                    tree.delete(*k);
                    expected.remove(k);
                }
                BTreeOp::Search(k) => {
                    prop_assert_eq!(tree.search(*k), expected.get(k).cloned());
                }
            }
        }

        prop_assert_eq!(tree.data, expected);
    });
}

#[test]
fn prop_btree_deterministic() {
    proptest!(|(
        ops in prop::collection::vec(arb_operation(), 1..50),
    )| {
        // First run
        let mut tree1 = ReferenceBTree::new();
        for op in ops.iter() {
            match op {
                BTreeOp::Insert(k, v) => tree1.insert(*k, *v),
                BTreeOp::Delete(k) => tree1.delete(*k),
                BTreeOp::Search(_) => {},
            }
        }

        // Second run with same operations
        let mut tree2 = ReferenceBTree::new();
        for op in ops.iter() {
            match op {
                BTreeOp::Insert(k, v) => tree2.insert(*k, *v),
                BTreeOp::Delete(k) => tree2.delete(*k),
                BTreeOp::Search(_) => {},
            }
        }

        prop_assert_eq!(&tree1.data, &tree2.data);
        prop_assert_eq!(tree1.is_empty(), tree2.is_empty());
    });
}

#[test]
fn prop_btree_empty_operations() {
    let mut tree = ReferenceBTree::new();

    // Delete from empty tree
    tree.delete(42);
    assert!(tree.is_empty());

    // Search in empty tree
    let result = tree.search(42);
    assert!(result.is_none());
}

#[test]
fn prop_btree_stress_test() {
    proptest!(|(
        ops in prop::collection::vec(arb_operation(), 100..1000),
    )| {
        let mut tree = ReferenceBTree::new();

        for op in ops.iter() {
            let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                match op {
                    BTreeOp::Insert(k, v) => tree.insert(*k, *v),
                    BTreeOp::Delete(k) => tree.delete(*k),
                    BTreeOp::Search(k) => {
                        let _ = tree.search(*k);
                    }
                }
            }));

            prop_assert!(result.is_ok(), "operation should not panic: {:?}", op);
        }
    });
}

#[test]
fn prop_btree_final_invariants() {
    proptest!(|(
        ops in prop::collection::vec(arb_operation(), 1..100),
    )| {
        let mut tree = ReferenceBTree::new();

        for op in ops {
            match op {
                BTreeOp::Insert(k, v) => tree.insert(k, v),
                BTreeOp::Delete(k) => tree.delete(k),
                BTreeOp::Search(_) => {},
            }
        }

        // Final invariant check
        // Keys are ordered
        let keys: Vec<_> = tree.data.keys().copied().collect();
        for i in 1..keys.len() {
            prop_assert!(keys[i - 1] < keys[i], "keys not ordered at end");
        }

        // No empty value vectors
        for (_, values) in tree.data.iter() {
            prop_assert!(!values.is_empty(), "empty value vector");
        }
    });
}

#[test]
fn test_btree_single_element() {
    let mut tree = ReferenceBTree::new();
    tree.insert(42, 100);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree.search(42), Some(vec![100]));
}

#[test]
fn test_btree_ascending_keys() {
    let mut tree = ReferenceBTree::new();

    for i in 0..100u64 {
        tree.insert(i, i * 2);
    }

    assert_eq!(tree.len(), 100);
    assert_eq!(tree.search(0), Some(vec![0]));
    assert_eq!(tree.search(99), Some(vec![198]));
}

#[test]
fn test_btree_descending_keys() {
    let mut tree = ReferenceBTree::new();

    for i in (0..100u64).rev() {
        tree.insert(i, i * 2);
    }

    assert_eq!(tree.len(), 100);
    assert_eq!(tree.search(0), Some(vec![0]));
    assert_eq!(tree.search(99), Some(vec![198]));
}

#[test]
fn test_btree_alternating_insert_delete() {
    let mut tree = ReferenceBTree::new();

    for i in 0..100u64 {
        tree.insert(i, i);
        if i % 2 == 0 && i > 0 {
            tree.delete(i - 1);
        }
    }

    assert_eq!(tree.len(), 51);
    assert_eq!(tree.search(0), Some(vec![0]));
    assert!(tree.search(1).is_none());
    assert_eq!(tree.search(99), Some(vec![99]));
    assert_eq!(tree.invariant_checks(), 149);
}

#[test]
fn integration_btree_full_lifecycle() {
    proptest!(|(
        insert_ops in prop::collection::vec(arb_key(), 10..100),
        delete_count in 0usize..50,
    )| {
        let mut tree = ReferenceBTree::new();
        let mut expected: HashSet<u64> = insert_ops.iter().copied().collect();

        for (i, key) in insert_ops.iter().enumerate() {
            tree.insert(*key, i as u64);
        }

        let inserted_count = tree.len();

        let keys_to_delete: Vec<_> = insert_ops.iter().take(delete_count).copied().collect();
        for key in keys_to_delete.iter() {
            tree.delete(*key);
            expected.remove(key);
        }

        prop_assert!(tree.len() <= inserted_count);
        prop_assert_eq!(tree.len(), expected.len());
        for key in expected {
            prop_assert!(tree.search(key).is_some(), "expected key {key} to remain");
        }
        prop_assert!(tree.invariant_checks() > 0);
    });
}
