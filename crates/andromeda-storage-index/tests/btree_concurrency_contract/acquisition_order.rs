use andromeda_storage_index::{BTreeConcurrencyPolicy, BTreeLatchLevel};

use crate::support::latch_target;

#[test]
fn acquisition_order_rejects_upward_and_reverse_sibling_latching() {
    let policy = BTreeConcurrencyPolicy::default();
    let root = latch_target(10, BTreeLatchLevel::Root, 0);
    let internal = latch_target(20, BTreeLatchLevel::Internal, 1);
    let leaf = latch_target(30, BTreeLatchLevel::Leaf, 2);
    let right_sibling = latch_target(31, BTreeLatchLevel::Sibling, 2);
    let left_sibling = latch_target(29, BTreeLatchLevel::Sibling, 2);

    assert!(policy.can_acquire_after(&[], root));
    assert!(policy.can_acquire_after(&[root], internal));
    assert!(policy.can_acquire_after(&[root, internal], leaf));

    assert!(!policy.can_acquire_after(&[root, internal, leaf], root));
    assert!(policy.can_acquire_after(&[root, internal, leaf], right_sibling));
    assert!(!policy.can_acquire_after(&[root, internal, leaf], left_sibling));
}
