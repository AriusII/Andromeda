use super::common::*;

/// **Verification: Batch version must advance (non-zero increment)**
///
/// This validates the infrastructure that ensures Alter/Drop always
/// advance the catalog version.
#[test]
fn test_batch_version_advancement_is_monotonic() {
    let base_version = CatalogVersion::new(5);
    let next_version = CatalogVersion::new(6);

    let batch = batch_with_create(base_version, 400, "test.Monotonic", next_version);
    let plan = batch.dry_run().expect("valid batch");

    assert_eq!(
        plan.next_version.get() - plan.previous_version.get(),
        1,
        "catalog version must advance by exactly 1 per batch"
    );

    assert!(
        plan.next_version.get() > plan.previous_version.get(),
        "next version must be greater than previous version"
    );
}
