use super::common::*;

/// **Verification of Replay Idempotency (DEC-023 requirement)**
///
/// When a batch is replayed (simulating recovery or audit):
/// - The same batch structure must produce the exact same plan
/// - The mutation records must be identical
/// - The version advancement must be deterministic
/// - No partial application or corruption can occur
///
/// This test verifies that batches are deterministic and can be safely replayed.
#[test]
fn test_batch_replay_idempotent() {
    let base_version = CatalogVersion::new(1);
    let next_version = CatalogVersion::new(2);

    let batch = batch_with_create(base_version, 200, "test.Idempotent", next_version);

    let plan1 = batch.dry_run().expect("first dry-run valid");
    let plan2 = batch.dry_run().expect("second dry-run valid");

    assert_eq!(plan1, plan2, "batch dry-run must be deterministic");

    assert_eq!(plan1.batch_id, plan2.batch_id);
    assert_eq!(plan1.database_id, plan2.database_id);
    assert_eq!(plan1.namespace_id, plan2.namespace_id);
    assert_eq!(plan1.previous_version, plan2.previous_version);
    assert_eq!(plan1.next_version, plan2.next_version);
    assert_eq!(plan1.operation_count, plan2.operation_count);
    assert_eq!(plan1.created_objects, plan2.created_objects);
    assert_eq!(plan1.deprecated_objects, plan2.deprecated_objects);
    assert_eq!(plan1.mutation_plan, plan2.mutation_plan);

    let records1 = plan1.mutation_plan.records();
    let records2 = plan2.mutation_plan.records();
    assert_eq!(
        records1, records2,
        "WAL records must be identical across replays"
    );

    assert_eq!(
        plan1.mutation_plan.record_count(),
        plan2.mutation_plan.record_count()
    );
}
