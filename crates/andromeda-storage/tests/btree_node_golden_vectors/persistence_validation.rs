use crate::support::{PAGE_SIZE, leaf_entry};
use andromeda_storage as storage;

#[test]
fn storage_facade_preserves_owner_key_order_validation() {
    let err = storage::BTreeNodeV1::new_leaf(
        storage::PageId::new(80),
        storage::Lsn::new(10),
        vec![leaf_entry(b"b", 10), leaf_entry(b"a", 11)],
        None,
        None,
        PAGE_SIZE,
    )
    .expect_err("descending keys must be rejected");
    assert!(err.message().contains("strictly ordered"));
}

#[test]
fn storage_facade_preserves_owner_internal_child_count_validation() {
    let err = storage::BTreeNodeV1::new_internal(
        storage::PageId::new(90),
        storage::Lsn::new(11),
        vec![b"k10".to_vec(), b"k20".to_vec()],
        vec![storage::PageId::new(100), storage::PageId::new(101)],
        PAGE_SIZE,
    )
    .expect_err("internal node requires keys + 1 children");
    assert!(err.message().contains("keys + 1"));
}
