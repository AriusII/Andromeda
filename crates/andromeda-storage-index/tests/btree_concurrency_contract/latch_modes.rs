use andromeda_storage_index::{BTreeConcurrencyPolicy, BTreeLatchMode, BTreeOperationKind};

#[test]
fn latch_modes_keep_reads_shared_and_mutations_exclusive_at_update_point() {
    assert_eq!(
        BTreeConcurrencyPolicy::descent_latch_mode(BTreeOperationKind::Lookup),
        BTreeLatchMode::Shared
    );
    assert_eq!(
        BTreeConcurrencyPolicy::descent_latch_mode(BTreeOperationKind::RangeScan),
        BTreeLatchMode::Shared
    );
    assert_eq!(
        BTreeConcurrencyPolicy::descent_latch_mode(BTreeOperationKind::Insert),
        BTreeLatchMode::Shared
    );
    assert_eq!(
        BTreeConcurrencyPolicy::mutation_latch_mode(BTreeOperationKind::Insert),
        BTreeLatchMode::Exclusive
    );
    assert_eq!(
        BTreeConcurrencyPolicy::mutation_latch_mode(BTreeOperationKind::Delete),
        BTreeLatchMode::Exclusive
    );
}
