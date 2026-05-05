#![forbid(unsafe_code)]

use andromeda_storage::{
    BTREE_DURABLE_FORMAT_PROMOTED, BTreeConcurrencyPolicy, BTreeLatchLevel, BTreeLatchMode,
    BTreeLatchTarget, BTreeMvccInteraction, BTreeOperationKind, BTreePanicPoisonBehavior,
    BTreeRestartReason, BTreeScanConsistency, PageId,
};

#[test]
fn btree_concurrency_policy_is_transient_and_dec032_safe() {
    let policy = BTreeConcurrencyPolicy::default();

    assert!(policy.validate().is_ok());
    assert!(!BTREE_DURABLE_FORMAT_PROMOTED);
    assert!(!policy.durable_format_promoted());
    assert_eq!(
        policy.scan_consistency,
        BTreeScanConsistency::StatementSnapshot
    );
    assert_eq!(
        policy.mvcc_interaction,
        BTreeMvccInteraction::LatchesProtectStructureOnly
    );
    assert_eq!(
        policy.panic_poison_behavior,
        BTreePanicPoisonBehavior::FailClosedReturnError
    );
}

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

#[test]
fn insert_restart_is_required_before_releasing_parent_for_full_child() {
    let branching_factor = 4;

    assert!(BTreeConcurrencyPolicy::child_safe_for_descent(
        BTreeOperationKind::Insert,
        2,
        branching_factor,
        false
    ));
    assert!(!BTreeConcurrencyPolicy::child_safe_for_descent(
        BTreeOperationKind::Insert,
        3,
        branching_factor,
        false
    ));
    assert_eq!(
        BTreeConcurrencyPolicy::restart_reason_for_unsafe_child(
            BTreeOperationKind::Insert,
            3,
            branching_factor,
            false
        ),
        Some(BTreeRestartReason::ChildMaySplit)
    );
}

#[test]
fn delete_restart_is_required_for_minimal_non_root_child() {
    let branching_factor = 8;

    assert!(BTreeConcurrencyPolicy::child_safe_for_descent(
        BTreeOperationKind::Delete,
        4,
        branching_factor,
        false
    ));
    assert!(!BTreeConcurrencyPolicy::child_safe_for_descent(
        BTreeOperationKind::Delete,
        3,
        branching_factor,
        false
    ));
    assert!(BTreeConcurrencyPolicy::child_safe_for_descent(
        BTreeOperationKind::Delete,
        0,
        branching_factor,
        true
    ));
    assert_eq!(
        BTreeConcurrencyPolicy::restart_reason_for_unsafe_child(
            BTreeOperationKind::Delete,
            3,
            branching_factor,
            false
        ),
        Some(BTreeRestartReason::ChildMayMergeOrRedistribute)
    );
}

#[test]
fn acquisition_order_rejects_upward_and_reverse_sibling_latching() {
    let policy = BTreeConcurrencyPolicy::default();
    let root = BTreeLatchTarget::new(PageId::new(10), BTreeLatchLevel::Root, 0);
    let internal = BTreeLatchTarget::new(PageId::new(20), BTreeLatchLevel::Internal, 1);
    let leaf = BTreeLatchTarget::new(PageId::new(30), BTreeLatchLevel::Leaf, 2);
    let right_sibling = BTreeLatchTarget::new(PageId::new(31), BTreeLatchLevel::Sibling, 2);
    let left_sibling = BTreeLatchTarget::new(PageId::new(29), BTreeLatchLevel::Sibling, 2);

    assert!(policy.can_acquire_after(&[], root));
    assert!(policy.can_acquire_after(&[root], internal));
    assert!(policy.can_acquire_after(&[root, internal], leaf));

    assert!(!policy.can_acquire_after(&[root, internal, leaf], root));
    assert!(policy.can_acquire_after(&[root, internal, leaf], right_sibling));
    assert!(!policy.can_acquire_after(&[root, internal, leaf], left_sibling));
}
