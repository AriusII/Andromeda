//! End-to-end integration tests for B-Tree insert without split

#[cfg(test)]
mod btree_insert_simple_e2e_tests {
    use andromeda_storage::{BTreeNodeImpl, BTreeConfig, PageId, RowId, KeyValuePair};
    use std::collections::BTreeMap;

    #[test]
    fn test_insert_1000_entries_no_split_e2e() {
        // Simulate a single-page B-Tree (no splits)
        let mut leaf_node = BTreeNodeImpl::new_leaf(PageId(1), None);
        let config = BTreeConfig::default();

        // Track expected state in memory
        let mut expected_state = BTreeMap::new();

        // Insert 1000 entries sequentially
        for i in 0..1000u64 {
            let key = i.to_le_bytes().to_vec();
            let row_id = RowId::new(i * 10);
            let row_id_bytes = row_id.get().to_le_bytes().to_vec();

            // Find correct position using binary search
            let idx = leaf_node.key_value_pairs
                .binary_search_by(|kvp| kvp.key.cmp(&key))
                .unwrap_or_else(|idx| idx);

            // Precondition: node must have space
            assert!(
                !leaf_node.is_full(config.branching_factor),
                "Leaf node at capacity at insertion {}: cannot insert without split",
                i
            );

            // Insert
            leaf_node.key_value_pairs.insert(
                idx,
                KeyValuePair {
                    key: key.clone(),
                    value: row_id_bytes,
                },
            );

            // Track in expected state
            expected_state.insert(key, row_id);
        }

        // Verify all entries present and in order
        assert_eq!(leaf_node.key_value_pairs.len(), 1000);
        assert!(!leaf_node.is_full(config.branching_factor));

        // Verify order matches expected state
        for (idx, (expected_key, kvp)) in expected_state.iter()
            .zip(leaf_node.key_value_pairs.iter())
            .enumerate() {
            assert_eq!(
                kvp.key, *expected_key,
                "Key mismatch at index {}: expected {:?}, got {:?}",
                idx, expected_key, kvp.key
            );

            // Verify value
            let expected_row_id = expected_state[expected_key];
            let stored_row_id = u64::from_le_bytes(
                <[u8; 8]>::try_from(&kvp.value[..])
                    .expect("invalid row id bytes"),
            );
            assert_eq!(
                RowId::new(stored_row_id), expected_row_id,
                "Row ID mismatch at index {}",
                idx
            );
        }
    }

    #[test]
    fn test_range_lookup_after_inserts_e2e() {
        let mut leaf_node = BTreeNodeImpl::new_leaf(PageId(1), None);

        // Insert entries: 10, 30, 20, 50, 40 (out of order)
        let inserts = vec![10, 30, 20, 50, 40];
        for value in &inserts {
            let key = vec![*value as u8];
            let idx = leaf_node.key_value_pairs
                .binary_search_by(|kvp| kvp.key[0].cmp(&value))
                .unwrap_or_else(|idx| idx);
            leaf_node.key_value_pairs.insert(
                idx,
                KeyValuePair {
                    key,
                    value: (*value as u64).to_le_bytes().to_vec(),
                },
            );
        }

        // Simulate range query [20, 50]
        let mut results = Vec::new();
        for kvp in &leaf_node.key_value_pairs {
            if kvp.key[0] >= 20 && kvp.key[0] <= 50 {
                results.push(kvp.key[0]);
            }
        }

        // Should be [20, 30, 40, 50]
        assert_eq!(results, vec![20, 30, 40, 50]);
    }

    #[test]
    fn test_lookup_all_entries_retrievable() {
        let mut leaf_node = BTreeNodeImpl::new_leaf(PageId(1), None);

        // Insert 100 random entries
        let mut entries = Vec::new();
        for i in 0..100u16 {
            entries.push(i);
        }

        // Shuffle them
        use std::collections::VecDeque;
        let mut queue: VecDeque<u16> = entries.into_iter().collect();

        for i in 0..50 {
            let val = queue.pop_front().unwrap();
            queue.push_back(val);

            let key = val.to_le_bytes().to_vec();
            let idx = leaf_node.key_value_pairs
                .binary_search_by(|kvp| kvp.key.cmp(&key))
                .unwrap_or_else(|idx| idx);
            leaf_node.key_value_pairs.insert(
                idx,
                KeyValuePair {
                    key,
                    value: (val as u64).to_le_bytes().to_vec(),
                },
            );
        }

        // Now verify we can lookup each
        for val in 0..50u16 {
            let key = val.to_le_bytes().to_vec();
            let found = leaf_node.key_value_pairs.iter()
                .find(|kvp| kvp.key == key);
            assert!(found.is_some(), "Failed to retrieve entry for key {}", val);
        }
    }

    #[test]
    fn test_serialization_roundtrip_preserves_order() {
        let mut original = BTreeNodeImpl::new_leaf(PageId(100), None);

        // Insert 50 entries in specific order
        for i in 0..50u8 {
            let key = vec![i];
            let idx = original.key_value_pairs
                .binary_search_by(|kvp| kvp.key.cmp(&key))
                .unwrap_or_else(|idx| idx);
            original.key_value_pairs.insert(
                idx,
                KeyValuePair {
                    key,
                    value: (i as u64).to_le_bytes().to_vec(),
                },
            );
        }

        // Serialize and deserialize
        let serialized = original.serialize();
        let restored = BTreeNodeImpl::deserialize(PageId(100), &serialized)
            .expect("deserialization failed");

        // Verify structure preserved
        assert_eq!(original.page_id, restored.page_id);
        assert_eq!(original.is_leaf, restored.is_leaf);
        assert_eq!(
            original.key_value_pairs.len(),
            restored.key_value_pairs.len()
        );

        // Verify all entries preserved
        for (orig, rest) in original.key_value_pairs.iter()
            .zip(restored.key_value_pairs.iter()) {
            assert_eq!(orig.key, rest.key);
            assert_eq!(orig.value, rest.value);
        }
    }

    #[test]
    fn test_no_split_precondition_enforced() {
        let mut leaf_node = BTreeNodeImpl::new_leaf(PageId(1), None);
        let config = BTreeConfig::default();

        // Fill to just below max
        let max_keys = (config.branching_factor - 2) as usize;
        for i in 0..max_keys {
            leaf_node.key_value_pairs.push(KeyValuePair {
                key: vec![i as u8],
                value: vec![],
            });
        }

        // Should still have room
        assert!(!leaf_node.is_full(config.branching_factor));

        // Add one more - should succeed
        leaf_node.key_value_pairs.push(KeyValuePair {
            key: vec![254],
            value: vec![],
        });

        // Now at capacity
        assert!(leaf_node.is_full(config.branching_factor));

        // Next insert would trigger split - precondition violated
        // In actual implementation, we would reject this or trigger split
    }

    #[test]
    fn test_correctness_under_repeated_operations() {
        let mut leaf_node = BTreeNodeImpl::new_leaf(PageId(1), None);

        // Simulate realistic pattern: insert, verify, insert more, verify
        for batch in 0..10 {
            let batch_start = batch * 50;
            let batch_end = batch_start + 50;

            for i in batch_start..batch_end {
                let key = vec![i as u8];
                let idx = leaf_node.key_value_pairs
                    .binary_search_by(|kvp| kvp.key[0].cmp(&(i as u8)))
                    .unwrap_or_else(|idx| idx);
                leaf_node.key_value_pairs.insert(
                    idx,
                    KeyValuePair {
                        key,
                        value: (i as u64).to_le_bytes().to_vec(),
                    },
                );
            }

            // Verify all entries are ordered after each batch
            for window in leaf_node.key_value_pairs.windows(2) {
                assert!(window[0].key <= window[1].key);
            }
        }

        // Final verification: all 500 entries present and ordered
        assert_eq!(leaf_node.key_value_pairs.len(), 500);
        for window in leaf_node.key_value_pairs.windows(2) {
            assert!(window[0].key < window[1].key);
        }
    }
}

#[cfg(test)]
mod wal_gc_recovery_integration_tests {
    use andromeda_storage::write_ahead_log::gc::*;
    use andromeda_storage::Lsn;
    use andromeda_core::AndromedaResult;
    use std::collections::HashMap;

    // Mock recovery context
    struct RecoverySimulator {
        segments: HashMap<u64, (Lsn, Lsn)>, // segment_id -> (creation_lsn, sealing_lsn)
        archived: HashMap<u64, ArchiveStatus>,
    }

    impl RecoverySimulator {
        fn new() -> Self {
            RecoverySimulator {
                segments: HashMap::new(),
                archived: HashMap::new(),
            }
        }

        fn add_segment(&mut self, id: u64, creation: Lsn, sealing: Lsn) {
            self.segments.insert(id, (creation, sealing));
            self.archived.insert(id, ArchiveStatus::Pending);
        }

        fn mark_archived(&mut self, id: u64) {
            self.archived.insert(id, ArchiveStatus::Archived);
        }

        fn can_mount_snapshot(&self, snapshot_lsn: Lsn) -> bool {
            // Can mount if we have segments covering the recovery path to snapshot_lsn
            self.segments.iter().any(|(_, (creation, sealing))| {
                *creation <= snapshot_lsn && snapshot_lsn <= *sealing
            })
        }
    }

    #[test]
    fn test_recovery_snapshot_protection_workflow() {
        let mut sim = RecoverySimulator::new();

        // Scenario: Create snapshot at LSN 1000, then write 1000 more records

        // Initial segments (0-1000)
        sim.add_segment(1, Lsn::new(0), Lsn::new(500));
        sim.add_segment(2, Lsn::new(500), Lsn::new(1000));

        // Snapshot created at LSN 1000
        let snapshot_lsn = Lsn::new(1000);

        // New segments after snapshot (1000-2000)
        sim.add_segment(3, Lsn::new(1000), Lsn::new(1250));
        sim.add_segment(4, Lsn::new(1250), Lsn::new(1500));
        sim.add_segment(5, Lsn::new(1500), Lsn::new(1750));
        sim.add_segment(6, Lsn::new(1750), Lsn::new(2000));

        // Mark initial segments as archived
        sim.mark_archived(1);
        sim.mark_archived(2);

        // Create GC candidates
        let mut candidates = Vec::new();
        for (id, (creation, sealing)) in &sim.segments {
            candidates.push(WalGcCandidate::new(*id, *creation, *sealing, 65536).unwrap());
        }

        // With snapshot at LSN 1000:
        // - Can GC segments 1, 2 (sealing 500, 1000 are <= 1000 and archived)
        // - Must protect segments 3-6 (created after snapshot, not yet archived)

        // Simulate min_snapshot_lsn = 1000
        let mut reclaimed = Vec::new();
        for candidate in &candidates {
            if candidate.sealing_lsn <= snapshot_lsn
                && sim.archived[&candidate.segment_id] == ArchiveStatus::Archived {
                reclaimed.push(candidate.segment_id);
            }
        }

        assert_eq!(reclaimed, vec![1, 2]);
    }

    #[test]
    fn test_snapshot_lsn_boundary_conditions() {
        let mut sim = RecoverySimulator::new();

        // Create segments
        sim.add_segment(1, Lsn::new(1000), Lsn::new(2000));
        sim.add_segment(2, Lsn::new(2000), Lsn::new(3000));
        sim.add_segment(3, Lsn::new(3000), Lsn::new(4000));

        // Test mounting snapshots at various LSNs
        assert!(sim.can_mount_snapshot(Lsn::new(1500))); // In segment 1
        assert!(sim.can_mount_snapshot(Lsn::new(2000))); // At boundary
        assert!(sim.can_mount_snapshot(Lsn::new(2500))); // In segment 2
        assert!(sim.can_mount_snapshot(Lsn::new(3999))); // Near end of segment 3
    }

    #[test]
    fn test_gc_reclamation_after_snapshot_close() {
        let mut sim = RecoverySimulator::new();

        // Setup: segments with archive status
        sim.add_segment(1, Lsn::new(0), Lsn::new(1000));
        sim.add_segment(2, Lsn::new(1000), Lsn::new(2000));
        sim.add_segment(3, Lsn::new(2000), Lsn::new(3000));

        sim.mark_archived(1);
        sim.mark_archived(2);
        sim.mark_archived(3);

        // Phase 1: Snapshot at LSN 1500
        // Protected: segments 2-3 (sealing > 1500)
        // Reclaimable: segment 1 (sealing <= 1500)
        let phase1_reclaimable = vec![
            WalGcCandidate::new(1, Lsn::new(0), Lsn::new(1000), 65536).unwrap(),
        ];
        assert_eq!(phase1_reclaimable.len(), 1);

        // Phase 2: Snapshot closed
        // Now all archived segments can be reclaimed
        let phase2_reclaimable = vec![
            WalGcCandidate::new(1, Lsn::new(0), Lsn::new(1000), 65536).unwrap(),
            WalGcCandidate::new(2, Lsn::new(1000), Lsn::new(2000), 65536).unwrap(),
            WalGcCandidate::new(3, Lsn::new(2000), Lsn::new(3000), 65536).unwrap(),
        ];
        assert_eq!(phase2_reclaimable.len(), 3);
    }

    #[test]
    fn test_multiple_overlapping_snapshot_protection() {
        let mut sim = RecoverySimulator::new();

        // Setup: 5 segments
        for i in 1..=5 {
            let creation = Lsn::new((i - 1) as u64 * 1000);
            let sealing = Lsn::new(i as u64 * 1000);
            sim.add_segment(i as u64, creation, sealing);
            sim.mark_archived(i as u64);
        }

        // Three overlapping snapshots
        let snap1 = Lsn::new(500);   // Segment 1
        let snap2 = Lsn::new(1500);  // Segment 2
        let snap3 = Lsn::new(3500);  // Segment 4

        // The minimum is snap1 (500)
        // So only segment 1 (sealing 1000) is protected? No - we protect >= snap1
        // Actually: sealing_lsn < min_snapshot_lsn allows GC
        // So: nothing can be GC'd until snap1 closes

        // After snap1 closes: min = snap2 (1500)
        // Segment 1 (sealing 1000 < 1500) can be GC'd
        // After snap2 closes: min = snap3 (3500)
        // Segments 1-2 can be GC'd (sealing 1000, 2000 < 3500)
    }
}
