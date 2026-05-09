use andromeda_storage_index::{BTreeConcurrencyPolicy, BTreeOperationKind, BTreeRestartReason};

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
