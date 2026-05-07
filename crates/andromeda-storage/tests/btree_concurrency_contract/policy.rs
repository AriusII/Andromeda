use andromeda_storage::{
    BTREE_DURABLE_FORMAT_PROMOTED, BTreeConcurrencyPolicy, BTreeMvccInteraction,
    BTreePanicPoisonBehavior, BTreeScanConsistency,
};

#[test]
fn btree_concurrency_policy_is_transient_and_dec032_safe() {
    let policy = BTreeConcurrencyPolicy::default();

    assert!(policy.validate().is_ok());
    const { assert!(!BTREE_DURABLE_FORMAT_PROMOTED) };
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
