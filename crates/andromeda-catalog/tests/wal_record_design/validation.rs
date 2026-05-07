use andromeda_catalog::{AlterCompatibilityPolicy, CatalogWalRecord};
use andromeda_error::AndromedaErrorKind;
use andromeda_types::{CatalogVersion, ContractHash};

use super::fixtures::{
    alter_procedure_record, apply_catalog_version_record, checkpoint_record,
    create_procedure_record, deprecate_procedure_record, drop_failure_reason,
    drop_procedure_record, object_id, test_hash,
};

#[test]
fn validation_rejects_zero_procedure_id_in_create() {
    assert_catalog_record_error(create_procedure_record(0, "proc", test_hash(1), vec![]));
}

#[test]
fn validation_rejects_empty_name_in_create() {
    assert_catalog_record_error(create_procedure_record(1, "", test_hash(1), vec![]));
}

#[test]
fn validation_rejects_zero_contract_hash_in_create() {
    assert_catalog_record_error(create_procedure_record(
        1,
        "proc",
        ContractHash::zero(),
        vec![],
    ));
}

#[test]
fn validation_rejects_zero_dependency_id_in_create() {
    assert_catalog_record_error(create_procedure_record(
        1,
        "proc",
        test_hash(1),
        vec![object_id(0)],
    ));
}

#[test]
fn validation_accepts_empty_dependencies_in_create() {
    let record = create_procedure_record(1, "proc", test_hash(1), vec![]);

    assert!(record.validate().is_ok());
}

#[test]
fn validation_rejects_invalid_alter_records() {
    for record in [
        alter_procedure_record(
            0,
            test_hash(1),
            test_hash(2),
            AlterCompatibilityPolicy::ExactHash,
        ),
        alter_procedure_record(
            1,
            ContractHash::zero(),
            test_hash(2),
            AlterCompatibilityPolicy::ExactHash,
        ),
        alter_procedure_record(
            1,
            test_hash(1),
            ContractHash::zero(),
            AlterCompatibilityPolicy::AdditiveOnly,
        ),
    ] {
        assert_catalog_record_error(record);
    }
}

#[test]
fn validation_rejects_invalid_deprecate_records() {
    assert_catalog_record_error(deprecate_procedure_record(0, "deprecated"));
    assert_catalog_record_error(deprecate_procedure_record(1, ""));
}

#[test]
fn validation_rejects_zero_procedure_id_in_drop() {
    assert_catalog_record_error(drop_procedure_record(0, None));
}

#[test]
fn validation_rejects_zero_batch_id_in_apply() {
    assert_catalog_record_error(apply_catalog_version_record(0, 1, 1, 100));
}

#[test]
fn validation_rejects_zero_version_in_apply() {
    assert_catalog_record_error(apply_catalog_version_record(1, 0, 1, 100));
}

#[test]
fn validation_rejects_zero_record_count_in_apply() {
    assert_catalog_record_error(apply_catalog_version_record(1, 1, 0, 100));
}

#[test]
fn validation_rejects_zero_lsn_in_apply() {
    assert_catalog_record_error(apply_catalog_version_record(1, 1, 1, 0));
}

#[test]
fn validation_rejects_invalid_checkpoint_records() {
    for record in [
        checkpoint_record(0, 1, 0),
        CatalogWalRecord::CatalogCheckpoint {
            checkpoint_lsn: 1,
            catalog_version: CatalogVersion::new(0),
            visible_procedure_count: 0,
        },
    ] {
        assert_catalog_record_error(record);
    }
}

#[test]
fn validation_rejects_zero_blocker_count_in_drop_failure() {
    let reason = drop_failure_reason(0, "some reason");

    assert_eq!(
        reason.validate().unwrap_err().kind(),
        AndromedaErrorKind::Catalog
    );
}

#[test]
fn validation_rejects_empty_description_in_drop_failure() {
    let reason = drop_failure_reason(1, "");

    assert_eq!(
        reason.validate().unwrap_err().kind(),
        AndromedaErrorKind::Catalog
    );
}

fn assert_catalog_record_error(record: CatalogWalRecord) {
    assert_eq!(
        record.validate().unwrap_err().kind(),
        AndromedaErrorKind::Catalog
    );
}
