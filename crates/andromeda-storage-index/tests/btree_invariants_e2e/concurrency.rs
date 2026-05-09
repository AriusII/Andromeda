use andromeda_storage_index::{BTreeConcurrencyPolicy, BTreeLatchLevel, BTreeLatchTarget, PageId};

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
