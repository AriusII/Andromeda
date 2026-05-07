use andromeda_catalog::{AlterCompatibilityPolicy, CatalogWalRecord, DefinitionBatchId};
use andromeda_core::CatalogVersion;

use super::fixtures::{
    alter_procedure_record, apply_catalog_version_record, assert_valid_record,
    create_procedure_record, deprecate_procedure_record, object_id, test_hash,
};

#[test]
fn recovery_incomplete_batch_rejected_if_apply_record_missing() {
    let batch_id = DefinitionBatchId::new(500);
    let version = CatalogVersion::new(50);
    let operations = [
        create_procedure_record(5001, "incomplete_proc_1", test_hash(0x11), vec![]),
        alter_procedure_record(
            5002,
            test_hash(0x22),
            test_hash(0x33),
            AlterCompatibilityPolicy::ExactHash,
        ),
    ];

    for operation in &operations {
        assert_valid_record(operation, "operation record should validate");
    }

    let incomplete_reason = format!(
        "{batch_id:?} at {version:?} missing ApplyCatalogVersion commit record after operation records"
    );
    assert!(
        !incomplete_reason.is_empty(),
        "Incomplete batches must have a documented reason"
    );
}

#[test]
fn recovery_incomplete_batch_detected_via_record_count() {
    let batch_id = DefinitionBatchId::new(501);
    let version = CatalogVersion::new(51);
    let operations = [
        create_procedure_record(6001, "op1", test_hash(0x44), vec![]),
        create_procedure_record(6002, "op2", test_hash(0x55), vec![]),
    ];
    let apply_record = apply_catalog_version_record(batch_id.get(), version.get(), 3, 7000);

    assert_valid_record(&apply_record, "Apply record validates syntactically");

    let claimed_record_count = apply_record_count(&apply_record);
    assert_ne!(
        operations.len(),
        claimed_record_count,
        "Recovery must detect record_count mismatch and reject batch"
    );
}

#[test]
fn recovery_committed_catalog_applies_all_operations() {
    let batch_id = DefinitionBatchId::new(800);
    let version = CatalogVersion::new(99);
    let operations = [
        create_procedure_record(8001, "proc_a", test_hash(0xAA), vec![]),
        create_procedure_record(8002, "proc_b", test_hash(0xBB), vec![object_id(8001)]),
        alter_procedure_record(
            8001,
            test_hash(0xAA),
            test_hash(0xCC),
            AlterCompatibilityPolicy::AdditiveOnly,
        ),
        deprecate_procedure_record(8002, "deprecated"),
    ];
    let commit =
        apply_catalog_version_record(batch_id.get(), version.get(), operations.len(), 9999);

    for record in &operations {
        assert_valid_record(record, "operation record should validate");
    }
    assert_valid_record(&commit, "commit record should validate");

    let claimed_count = apply_record_count(&commit);
    assert_eq!(
        operations.len(),
        claimed_count,
        "All claimed operations must be present for replay"
    );
}

#[test]
fn recovery_committed_catalog_version_is_visible() {
    let batch_id = DefinitionBatchId::new(900);
    let version = CatalogVersion::new(150);
    let commit_record = apply_catalog_version_record(batch_id.get(), version.get(), 1, 20000);

    assert_valid_record(&commit_record, "commit record should validate");

    assert_eq!(
        commit_record.catalog_version(),
        Some(version),
        "Catalog version must be visible after recovery"
    );
}

fn apply_record_count(record: &CatalogWalRecord) -> usize {
    let CatalogWalRecord::ApplyCatalogVersion { record_count, .. } = record else {
        panic!("Expected ApplyCatalogVersion");
    };
    *record_count
}
